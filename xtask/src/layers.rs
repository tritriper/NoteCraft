//! `cargo xtask layers`: the crate layering rules (AGENTS.md, "This repository").
//!
//! Rule A: a workspace crate depends only on workspace crates of a strictly lower layer (every
//! dependency kind counts; a crate's dev-dependency on itself is fine).
//! Rule B: only the crates in [`UI_ALLOWED`] may use a UI toolkit crate ([`UI_CRATES`]), whether
//! directly (any dependency kind) or transitively through normal and build dependencies.
//! Rule C: no crate in L1 to L4 reaches a network crate ([`NET_CRATES`]) through normal or build
//! dependencies, directly or transitively (ADR 0005 #9: no request below L5).
//!
//! The rules run on a small graph model so they can be unit-tested; [`from_metadata`] builds it
//! from `cargo metadata`.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// Where a workspace crate sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// A layered library crate, L0 (lowest) to L6 (the UI).
    Layer(u8),
    /// A binary or app shell: above every layer, may use any layered crate, not another app.
    App,
    /// Build tooling (xtask): exempt from the rules.
    Tool,
}

/// The layering table, by package name without the `notecraft-` prefix. A workspace crate
/// missing from it is an error: add it here when you add a crate.
pub const TABLE: &[(&str, Class)] = &[
    ("model", Class::Layer(1)),
    ("md", Class::Layer(2)),
    ("query", Class::Layer(2)),
    ("store", Class::Layer(3)),
    ("search", Class::Layer(3)),
    ("engine", Class::Layer(4)),
    ("mcp", Class::Layer(5)),
    ("ui-egui", Class::Layer(6)),
    ("cli", Class::App),
    ("xtask", Class::Tool),
];

/// External crates that are a UI toolkit or windowing layer. Entries ending in `*` are prefixes.
pub const UI_CRATES: &[&str] = &["egui*", "eframe", "winit", "rfd"];

/// External crates that open sockets or speak a network protocol. Entries ending in `*` are
/// prefixes. Rule C keeps them out of L1 to L4.
pub const NET_CRATES: &[&str] = &[
    "reqwest*",
    "hyper*",
    "ureq",
    "curl*",
    "isahc",
    "attohttpc",
    "surf",
    "h2",
    "h3*",
    "quinn*",
    "tungstenite*",
    "tokio-tungstenite",
    "socket2",
    "mio",
    "tokio",
    "async-std",
    "native-tls",
    "rustls*",
    "openssl*",
    "web-sys",
];

/// The highest layer that rule C covers.
const NET_FREE_UP_TO: u8 = 4;

/// Workspace crates (short names) allowed to use [`UI_CRATES`]. Desktop and web app shells that
/// embed the UI join this list when they land (US-L0-07).
pub const UI_ALLOWED: &[&str] = &["ui-egui"];

const PREFIX: &str = "notecraft-";

pub fn short_name(package: &str) -> &str {
    package.strip_prefix(PREFIX).unwrap_or(package)
}

pub fn classify(package: &str) -> Option<Class> {
    let short = short_name(package);
    TABLE.iter().find(|(name, _)| *name == short).map(|(_, class)| *class)
}

fn matches_any(patterns: &[&str], name: &str) -> bool {
    patterns.iter().any(|pattern| match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == *pattern,
    })
}

fn is_ui_crate(name: &str) -> bool {
    matches_any(UI_CRATES, name)
}

fn is_net_crate(name: &str) -> bool {
    matches_any(NET_CRATES, name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepKind {
    Normal,
    Build,
    Dev,
}

/// One package of the resolved dependency graph.
#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub workspace: bool,
    /// Dependencies as (index into [`Graph::nodes`], kind); a dependency used with two kinds
    /// appears twice.
    pub deps: Vec<(usize, DepKind)>,
}

#[derive(Debug, Clone, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    /// A workspace crate the table does not know.
    Unregistered { krate: String },
    /// Rule A.
    Upward { krate: String, from: String, dep: String, to: String, kind: DepKind },
    /// Rule B: `path` runs from the offending workspace crate to the UI crate.
    Ui { krate: String, path: Vec<String> },
    /// Rule C: `path` runs from the offending workspace crate to the network crate.
    Net { krate: String, path: Vec<String> },
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Violation::Unregistered { krate } => write!(f, "{krate}: not in the layer table; add it to TABLE in xtask/src/layers.rs"),
            Violation::Upward { krate, from, dep, to, kind } => {
                write!(f, "{krate} ({from}) -> {dep} ({to}) [{kind:?}]: a crate may only depend on crates of a strictly lower layer")
            }
            Violation::Ui { krate, path } => {
                write!(f, "{krate} reaches a UI toolkit via {}: only {} may use one", path.join(" -> "), UI_ALLOWED.join(", "))
            }
            Violation::Net { krate, path } => {
                write!(f, "{krate} reaches a network crate via {}: nothing below L5 may use the network", path.join(" -> "))
            }
        }
    }
}

fn describe(class: Class) -> String {
    match class {
        Class::Layer(l) => format!("L{l}"),
        Class::App => "app".to_owned(),
        Class::Tool => "tool".to_owned(),
    }
}

/// Rule A for one edge: may a crate of class `from` depend on a crate of class `to`?
fn allowed(from: Class, to: Class) -> bool {
    match (from, to) {
        (Class::Tool, _) => true,
        (Class::App, Class::Layer(_)) => true,
        (Class::Layer(f), Class::Layer(t)) => t < f,
        _ => false,
    }
}

pub fn check(graph: &Graph) -> Vec<Violation> {
    let mut violations = Vec::new();
    for (index, node) in graph.nodes.iter().enumerate() {
        if !node.workspace {
            continue;
        }
        let Some(class) = classify(&node.name) else {
            violations.push(Violation::Unregistered { krate: node.name.clone() });
            continue;
        };
        if class == Class::Tool {
            continue;
        }
        // Rule A.
        for &(dep, kind) in &node.deps {
            let Some(target) = graph.nodes.get(dep) else { continue };
            if !target.workspace || dep == index {
                continue;
            }
            // An unregistered dependency is reported on its own entry.
            let Some(to) = classify(&target.name) else { continue };
            if !allowed(class, to) {
                violations.push(Violation::Upward {
                    krate: node.name.clone(),
                    from: describe(class),
                    dep: target.name.clone(),
                    to: describe(to),
                    kind,
                });
            }
        }
        // Rule B.
        if !UI_ALLOWED.contains(&short_name(&node.name))
            && let Some(path) = ui_path(graph, index)
        {
            violations.push(Violation::Ui { krate: node.name.clone(), path });
        }
        // Rule C.
        if matches!(class, Class::Layer(l) if l <= NET_FREE_UP_TO)
            && let Some(path) = net_path(graph, index)
        {
            violations.push(Violation::Net { krate: node.name.clone(), path });
        }
    }
    violations
}

/// The shortest path from `start` to a UI crate: its direct dependencies of any kind, then
/// normal and build dependencies only (a dependency's dev-dependencies are not built for us).
fn ui_path(graph: &Graph, start: usize) -> Option<Vec<String>> {
    let mut parent: HashMap<usize, usize> = HashMap::new();
    let mut queue = VecDeque::from([start]);
    while let Some(current) = queue.pop_front() {
        let node = graph.nodes.get(current)?;
        for &(dep, kind) in &node.deps {
            if kind == DepKind::Dev && current != start {
                continue;
            }
            if dep == start || parent.contains_key(&dep) {
                continue;
            }
            parent.insert(dep, current);
            let Some(target) = graph.nodes.get(dep) else { continue };
            if is_ui_crate(&target.name) {
                return Some(walk_back(graph, &parent, start, dep));
            }
            queue.push_back(dep);
        }
    }
    None
}

/// The shortest path from `start` to a network crate through normal and build dependencies.
fn net_path(graph: &Graph, start: usize) -> Option<Vec<String>> {
    let mut parent: HashMap<usize, usize> = HashMap::new();
    let mut queue = VecDeque::from([start]);
    while let Some(current) = queue.pop_front() {
        let node = graph.nodes.get(current)?;
        for &(dep, kind) in &node.deps {
            if kind == DepKind::Dev || dep == start || parent.contains_key(&dep) {
                continue;
            }
            parent.insert(dep, current);
            let Some(target) = graph.nodes.get(dep) else { continue };
            if !target.workspace && is_net_crate(&target.name) {
                return Some(walk_back(graph, &parent, start, dep));
            }
            queue.push_back(dep);
        }
    }
    None
}

fn walk_back(graph: &Graph, parent: &HashMap<usize, usize>, start: usize, end: usize) -> Vec<String> {
    let mut path = Vec::new();
    let mut current = end;
    // Bounded by the node count: each step moves to a node discovered earlier.
    for _ in 0..=graph.nodes.len() {
        path.push(graph.nodes.get(current).map_or_else(|| "?".to_owned(), |n| n.name.clone()));
        if current == start {
            break;
        }
        match parent.get(&current) {
            Some(&p) => current = p,
            None => break,
        }
    }
    path.reverse();
    path
}

/// Builds the graph from `cargo metadata --format-version 1` (with its `resolve` section).
pub fn from_metadata(meta: &Value) -> Result<Graph, String> {
    let packages = meta["packages"].as_array().ok_or("cargo metadata: no `packages` array")?;
    let members: Vec<&str> =
        meta["workspace_members"].as_array().ok_or("cargo metadata: no `workspace_members` array")?.iter().filter_map(Value::as_str).collect();
    let mut index: HashMap<&str, usize> = HashMap::new();
    let mut graph = Graph::default();
    for package in packages {
        let id = package["id"].as_str().ok_or("cargo metadata: a package without an id")?;
        let name = package["name"].as_str().ok_or("cargo metadata: a package without a name")?;
        index.insert(id, graph.nodes.len());
        graph.nodes.push(Node { name: name.to_owned(), workspace: members.contains(&id), deps: Vec::new() });
    }
    let nodes = meta["resolve"]["nodes"].as_array().ok_or("cargo metadata: no `resolve.nodes` (run without --no-deps)")?;
    for node in nodes {
        let id = node["id"].as_str().ok_or("cargo metadata: a resolve node without an id")?;
        let Some(&from) = index.get(id) else { continue };
        let mut deps = Vec::new();
        for dep in node["deps"].as_array().map(Vec::as_slice).unwrap_or_default() {
            let Some(&to) = dep["pkg"].as_str().and_then(|pkg| index.get(pkg)) else { continue };
            for kind in dep["dep_kinds"].as_array().map(Vec::as_slice).unwrap_or_default() {
                let kind = match kind["kind"].as_str() {
                    Some("dev") => DepKind::Dev,
                    Some("build") => DepKind::Build,
                    _ => DepKind::Normal,
                };
                deps.push((to, kind));
            }
        }
        if let Some(node) = graph.nodes.get_mut(from) {
            node.deps = deps;
        }
    }
    Ok(graph)
}

pub fn run(root: &Path) -> Result<(), String> {
    let out = Command::new(crate::cargo())
        .current_dir(root)
        .args(["metadata", "--format-version", "1"])
        .output()
        .map_err(|e| format!("cannot run `cargo metadata`: {e}"))?;
    if !out.status.success() {
        return Err(format!("`cargo metadata` failed ({}): {}", out.status, String::from_utf8_lossy(&out.stderr).trim()));
    }
    let meta: Value = serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata: invalid JSON: {e}"))?;
    let graph = from_metadata(&meta)?;
    let violations = check(&graph);
    let members = graph.nodes.iter().filter(|n| n.workspace).count();
    if violations.is_empty() {
        println!("layers: {members} workspace crates, no layering violation");
        Ok(())
    } else {
        let list: Vec<String> = violations.iter().map(ToString::to_string).collect();
        Err(format!("{} layering violation(s):\n  {}", list.len(), list.join("\n  ")))
    }
}

#[cfg(test)]
mod tests {
    use super::DepKind::{Build, Dev, Normal};
    use super::*;

    /// (name, workspace member?, dependencies by name and kind).
    type Spec<'a> = (&'a str, bool, &'a [(&'a str, DepKind)]);

    fn graph(spec: &[Spec<'_>]) -> Graph {
        let position = |name: &str| spec.iter().position(|(n, _, _)| *n == name).unwrap();
        Graph {
            nodes: spec
                .iter()
                .map(|(name, workspace, deps)| Node {
                    name: (*name).to_owned(),
                    workspace: *workspace,
                    deps: deps.iter().map(|(d, k)| (position(d), *k)).collect(),
                })
                .collect(),
        }
    }

    #[test]
    fn downward_graph_passes() {
        let g = graph(&[
            ("notecraft-model", true, &[]),
            ("notecraft-md", true, &[("notecraft-model", Normal)]),
            ("notecraft-engine", true, &[("notecraft-md", Normal), ("notecraft-model", Normal)]),
            ("notecraft-cli", true, &[("notecraft-engine", Normal)]),
            ("xtask", true, &[("notecraft-cli", Normal)]),
        ]);
        assert!(check(&g).is_empty());
    }

    #[test]
    fn upward_and_sideways_dependencies_flagged() {
        let g = graph(&[
            ("notecraft-model", true, &[("notecraft-engine", Normal)]),
            ("notecraft-md", true, &[("notecraft-query", Dev)]),
            ("notecraft-query", true, &[]),
            ("notecraft-engine", true, &[]),
        ]);
        let v = check(&g);
        assert_eq!(v.len(), 2, "{v:?}");
        assert!(v.iter().any(|x| matches!(x, Violation::Upward { krate, kind: Normal, .. } if krate == "notecraft-model")));
        assert!(v.iter().any(|x| matches!(x, Violation::Upward { krate, kind: Dev, .. } if krate == "notecraft-md")));
    }

    #[test]
    fn app_may_not_depend_on_app_and_layers_may_not_depend_on_apps() {
        let g = graph(&[("notecraft-cli", true, &[]), ("notecraft-mcp", true, &[("notecraft-cli", Normal)])]);
        assert!(matches!(check(&g)[..], [Violation::Upward { .. }]));
    }

    #[test]
    fn self_dev_dependency_ignored() {
        let g = graph(&[("notecraft-model", true, &[("notecraft-model", Dev)])]);
        assert!(check(&g).is_empty());
    }

    #[test]
    fn unregistered_workspace_crate_is_an_error() {
        let g = graph(&[("notecraft-sync", true, &[])]);
        assert!(matches!(&check(&g)[..], [Violation::Unregistered { krate }] if krate == "notecraft-sync"));
    }

    #[test]
    fn ui_toolkit_below_the_ui_flagged_directly_and_transitively() {
        let g = graph(&[
            ("notecraft-engine", true, &[("helper", Normal)]),
            ("notecraft-mcp", true, &[("egui_kittest", Dev)]),
            ("notecraft-ui-egui", true, &[("eframe", Normal)]),
            ("helper", false, &[("winit", Build)]),
            ("winit", false, &[]),
            ("eframe", false, &[]),
            ("egui_kittest", false, &[]),
        ]);
        let v = check(&g);
        assert_eq!(v.len(), 2, "{v:?}");
        assert!(v.iter().any(|x| matches!(x, Violation::Ui { krate, path } if krate == "notecraft-engine" && path.len() == 3)));
        assert!(v.iter().any(|x| matches!(x, Violation::Ui { krate, .. } if krate == "notecraft-mcp")));
    }

    #[test]
    fn a_dependency_dev_dependency_on_a_toolkit_is_not_ours() {
        let g = graph(&[("notecraft-model", true, &[("helper", Normal)]), ("helper", false, &[("rfd", Dev)]), ("rfd", false, &[])]);
        assert!(check(&g).is_empty());
    }

    #[test]
    fn rule_c_network_crate_below_l5_is_refused() {
        let g = graph(&[
            ("notecraft-store", true, &[("ureq", Normal)]),
            ("ureq", false, &[]),
            ("notecraft-model", true, &[("reqwest-blocking", Build)]),
            ("reqwest-blocking", false, &[]),
        ]);
        let v = check(&g);
        assert_eq!(v.len(), 2, "{v:?}");
        assert!(v.iter().all(|x| matches!(x, Violation::Net { .. })));
    }

    #[test]
    fn rule_c_network_crate_in_l5_or_app_is_allowed() {
        let g = graph(&[
            ("notecraft-mcp", true, &[("tokio", Normal)]),
            ("notecraft-cli", true, &[("hyper", Normal)]),
            ("notecraft-ui-egui", true, &[("socket2", Normal)]),
            ("tokio", false, &[]),
            ("hyper", false, &[]),
            ("socket2", false, &[]),
        ]);
        assert!(check(&g).is_empty());
    }

    #[test]
    fn rule_c_transitive_network_crate_is_refused() {
        let g = graph(&[
            ("notecraft-engine", true, &[("helper", Normal)]),
            ("helper", false, &[("deeper", Normal)]),
            ("deeper", false, &[("rustls-pemfile", Build)]),
            ("rustls-pemfile", false, &[]),
        ]);
        let v = check(&g);
        assert!(matches!(&v[..], [Violation::Net { krate, path }] if krate == "notecraft-engine" && path.len() == 4), "{v:?}");
    }

    #[test]
    fn rule_c_ignores_dev_dependencies_and_lookalike_names() {
        let g = graph(&[("notecraft-engine", true, &[("tokio", Dev), ("tokio-util", Normal)]), ("tokio", false, &[]), ("tokio-util", false, &[])]);
        assert!(check(&g).is_empty());
    }

    #[test]
    fn metadata_parsing() {
        let meta = serde_json::json!({
            "packages": [
                {"id": "a 0.1.0", "name": "notecraft-model"},
                {"id": "b 0.1.0", "name": "notecraft-md"},
                {"id": "c 1.0.0", "name": "serde"}
            ],
            "workspace_members": ["a 0.1.0", "b 0.1.0"],
            "resolve": {"nodes": [
                {"id": "a 0.1.0", "deps": []},
                {"id": "b 0.1.0", "deps": [
                    {"name": "notecraft_model", "pkg": "a 0.1.0", "dep_kinds": [{"kind": null, "target": null}]},
                    {"name": "serde", "pkg": "c 1.0.0", "dep_kinds": [{"kind": null, "target": null}, {"kind": "dev", "target": null}]}
                ]},
                {"id": "c 1.0.0", "deps": []}
            ]}
        });
        let g = from_metadata(&meta).unwrap();
        assert_eq!(g.nodes.iter().filter(|n| n.workspace).count(), 2);
        assert_eq!(g.nodes[1].deps, vec![(0, Normal), (2, Normal), (2, Dev)]);
        assert!(check(&g).is_empty());
    }
}
