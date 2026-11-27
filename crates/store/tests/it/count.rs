//! `Workspace::count_items` (gate-1.2, ADR 0005 #13): pages, databases and rows, the trash left out.

use notecraft_store::{Project, RelPath, Workspace};

use crate::support::*;
use crate::sweep::model;

const FM: &str = "---\ntitle: t\n---\nbody\n";
const TRASHED: &str = "---\ntitle: t\ntrashed: 2026-11-27T00:00:00Z\n---\nbody\n";

fn rel(s: &str) -> RelPath {
    RelPath::parse(s).unwrap()
}

fn fresh(dir: &TempDir) -> Workspace {
    Workspace::create(&Project::new(dir.path()), model("n")).unwrap()
}

fn put_all(ws: &Workspace, files: &[(&str, &str)]) {
    for (p, c) in files {
        ws.write(&rel(p), c.as_bytes()).unwrap();
    }
}

fn counts(ws: &Workspace) -> (u64, u64, u64) {
    let c = ws.count_items().unwrap();
    (c.pages, c.databases, c.rows)
}

// AT-L0-03-a (rows), commands.md §2: pages exclude rows and database pages, the trash is not counted.
#[test]
fn count_items_counts_pages_databases_and_rows_and_skips_the_trash() {
    let dir = TempDir::new();
    let ws = fresh(&dir);
    put_all(
        &ws,
        &[
            ("pages/a.md", FM),
            ("pages/a/db-1/database.md", FM),
            ("pages/a/db-1/rows/r1.md", FM),
            ("pages/a/db-1/rows/r2.md", FM),
            ("pages/a/db-1/rows/r1/child.md", FM),
            ("pages/a/db-1/views/v.json", "{}"),
            ("pages/root-db/database.md", FM),
            ("pages/root-db/rows/r3.md", FM),
            ("pages/t.md", TRASHED),
            ("pages/t/under.md", FM),
            ("pages/a/db-1/rows/r4.md", TRASHED),
            ("pages/a/db-1/rows/r4/c.md", FM),
            ("pages/gone-db/database.md", TRASHED),
            ("pages/gone-db/rows/r5.md", FM),
            ("pages/x/rows/not-a-row.md", FM),
            ("pages/body.md", "---\ntitle: b\n---\ntrashed: x\n"),
        ],
    );
    // pages: a, child, not-a-row, body; databases: db-1, root-db; rows: r1, r2, r3.
    assert_eq!(counts(&ws), (4, 2, 3));

    // Trashing a page drops everything below it, whatever its own state.
    ws.write(&rel("pages/a.md"), TRASHED.as_bytes()).unwrap();
    assert_eq!(counts(&ws), (2, 1, 1));

    // Restoring brings it back.
    ws.write(&rel("pages/a.md"), FM.as_bytes()).unwrap();
    assert_eq!(counts(&ws), (4, 2, 3));
}

// commands.md §2: only a top-level `trashed` key of the front matter trashes an item.
#[test]
fn trashed_is_a_top_level_front_matter_key() {
    let trashed = [
        ("plain", "---\ntrashed: 2026-01-01\n---\nx"),
        ("double quoted key", "---\n\"trashed\": true\n---\nx"),
        ("single quoted key", "---\n'trashed': true\n---\nx"),
        ("crlf", "---\r\ntitle: t\r\ntrashed: 2026-01-01\r\n---\r\nx"),
        ("bom", "\u{feff}---\ntrashed: 2026-01-01\n---\nx"),
        ("dots terminator", "---\ntrashed: 2026-01-01\n...\nx"),
    ];
    for (label, text) in trashed {
        let dir = TempDir::new();
        let ws = fresh(&dir);
        put_all(&ws, &[("pages/p.md", text), ("pages/q.md", FM)]);
        assert_eq!(counts(&ws), (1, 0, 0), "{label} must be trashed");
    }
    let kept = [
        ("nested key", "---\nmeta:\n  trashed: x\n---\nx"),
        ("longer key", "---\ntrashed_at: 2026-01-01\n---\nx"),
        ("after the front matter", "---\ntitle: t\n---\ntrashed: x\n---\n"),
        ("no front matter", "trashed: x\n"),
        ("unclosed", "---\ntrashed: x\n"),
        ("not on the first line", "\n---\ntrashed: x\n---\n"),
    ];
    for (label, text) in kept {
        let dir = TempDir::new();
        let ws = fresh(&dir);
        put_all(&ws, &[("pages/p.md", text)]);
        assert_eq!(counts(&ws), (1, 0, 0), "{label} must be counted");
    }
}

// ADR 0005 #13: a `rows/` folder is rows only inside a database; trashing the database hides them.
#[test]
fn a_trashed_database_hides_its_rows_and_a_rows_folder_without_database_is_pages() {
    let dir = TempDir::new();
    let ws = fresh(&dir);
    put_all(&ws, &[("pages/d/database.md", FM), ("pages/d/rows/r.md", FM), ("pages/e/rows/r.md", FM)]);
    assert_eq!(counts(&ws), (1, 1, 1));
    ws.write(&rel("pages/d/database.md"), TRASHED.as_bytes()).unwrap();
    assert_eq!(counts(&ws), (1, 0, 0));
}
