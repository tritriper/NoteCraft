# NoteCraft — méthodo de développement par sous-agents

Phase 2 du projet : une fois `spec/` validée (CdC → use cases → lots → US), chaque
user story est développée par une chaîne d'agents spécialisés dans Claude Code.
Ce document décrit les rôles, le cycle de vie d'une US, les règles de branches, et
donne en annexe les fichiers `.claude/agents/*.md` prêts à poser dans le dépôt.

---

## 1. Principes

1. **Deux salles, un mur.** La salle A (spec, seule à utiliser REA) et la salle B
   (dev) sont deux dépôts ou dossiers distincts. Le dépôt de dev ne contient qu'une
   copie de `spec/`. Aucun agent de la salle B ne lit `plan/rea/` ni la salle A.
2. **Une US = une branche = un worktree = un agent dev.** Les agents dev travaillent
   en parallèle dans des worktrees isolés, chacun sur les crates que le lead tech lui
   a attribuées (règle WordCraft : un agent par crate, `CARGO_TARGET_DIR` séparé).
3. **Celui qui écrit ne valide pas.** Les tests d'acceptation sont écrits par un autre
   agent que celui qui code, la revue par un agent au contexte neuf, la fusion par un
   troisième.
4. **Les règles de la suite Craft sont des portes, pas des conseils.** Sans
   `cargo xtask ci` vert, pas de revue. Sans revue approuvée, pas de fusion.
5. **L'humain garde les décisions publiques.** Les agents ne poussent jamais vers un
   remote, n'ouvrent ni PR ni issue upstream. Toi seul le fais, après une revue de
   lot.

## 2. Les rôles

| Rôle | Qui | Ce qu'il fait | Ce qu'il n'a pas le droit de faire | Modèle conseillé |
|---|---|---|---|---|
| **Product owner** | Toi | Valide CdC, lots, priorités ; accepte un lot fini ; pousse et ouvre les PR upstream | — | — |
| **Orchestrateur** | Session Claude Code principale | Lit `plan/STATUS.md`, choisit les US prêtes, lance les sous-agents, tient le statut à jour, remonte les blocages | Écrire du code produit | inherit |
| **Spec (salle A)** | Session séparée, autre dossier | Écrit et amende `spec/` ; seul à utiliser REA | Écrire du code ; voir le dépôt de dev | opus |
| **Lead tech** | Sous-agent | Architecture (crates, couches L0–L6), ADR, découpe une US en tâches, attribue les crates, tranche les désaccords dev/review | Implémenter une US ; fusionner | opus |
| **Testeur** | Sous-agent | Écrit les tests d'acceptation d'une US *avant* le dev, à partir des Given/When/Then, et plante des mutants pour prouver qu'ils échouent | Écrire le code de production | sonnet |
| **Dev** | Sous-agent, un par US, worktree isolé | Implémente l'US jusqu'à ce que les tests passent ; `cargo xtask ci` vert ; met à jour `NOTES.md` de l'US | Modifier les tests d'acceptation (il demande au testeur) ; toucher une crate non attribuée | sonnet |
| **Reviewer** | Sous-agent, lecture seule, contexte neuf | Relit le diff contre une checklist (spec, never-crash, couches, clean-room, tests) ; approuve ou demande des changements | Modifier le code | opus |
| **Auditeur clean-room** | Sous-agent, lecture seule | Passe le diff au crible du mur : marques tierces, termes de la denylist REA, textes copiés, assets non attribués | Modifier le code | haiku |
| **Intégrateur (« master »)** | Sous-agent | Fusionne en série les branches approuvées dans `develop`, rebase, relance la CI complète, met à jour `ROADMAP.md` et la parité | Résoudre autre chose qu'un conflit trivial ; pousser vers un remote | sonnet |

Le testeur et l'auditeur sont optionnels au départ, mais ce sont eux qui donnent de la
rigueur à la chaîne : sans testeur, le dev écrit ses propres tests et se note lui-même.

## 3. Cycle de vie d'une US

```
Backlog ──(DoR)──> Prête ──> Tests écrits ──> En dev ──> En revue ──> Approuvée ──> Intégrée ──> Acceptée (lot)
                                   ▲              │           │
                                   │              └─ CI rouge ┘ (retour dev)
                                   └──── demande de changement de test (via lead tech)
```

**Definition of Ready (DoR)** — une US ne part en dev que si :
- elle a ses critères d'acceptation Given/When/Then et ses cas d'erreur ;
- ses dépendances (autres US) sont intégrées ;
- le lead tech a indiqué les crates touchées et l'agent qui les possède ;
- elle est de taille S ou M (une L est redécoupée).

**Definition of Done (DoD)** — une US est intégrée si :
- tous ses tests d'acceptation passent et chaque mutant planté échoue ;
- `cargo xtask ci` est vert (fmt, clippy `-D warnings`, tests, assets, layers, wasm) ;
- aucun `unwrap`/`expect`/`panic!` hors tests, test de régression pour chaque crash corrigé ;
- les commandes nouvelles sont des `CommandSpec` testées et apparaissent dans la parité ;
- revue approuvée et audit clean-room passé ;
- capture d'écran jointe si l'UI change ;
- `NOTES.md` de l'US et `traceability.md` à jour.

**Boucles bornées.** Au plus 3 allers-retours dev ↔ reviewer. Au-delà, l'orchestrateur
escalade au lead tech, qui tranche ou redécoupe l'US. Si la spec est ambiguë, l'US
repasse en « Backlog » avec une question dans `spec/open-questions.md` : seule la
salle A modifie la spec.

## 4. Branches, worktrees et commits

- `main` : reflète ce qui est prêt à proposer upstream. Seul toi y fusionnes.
- `develop` : branche d'intégration, alimentée uniquement par l'intégrateur.
- `us/L2-04-relation-property` : une branche par US, créée depuis `develop`, dans un
  worktree isolé (`isolation: worktree`).
- Commits au format Craft avec l'id de tâche : `US-L2-04: relation property picker`.
- Fusion en série (une branche à la fois, rebase sur `develop` puis CI complète),
  pour que chaque état de `develop` soit vert.
- Chaque agent dev utilise son propre `CARGO_TARGET_DIR` et le supprime à la fin
  (règle WordCraft, sinon le disque sature vite avec plusieurs worktrees Rust).

## 5. Fichiers de pilotage (dans `plan/`, gitignoré comme dans WordCraft)

- `plan/STATUS.md` : tableau des US (id, lot, statut, agent, branche, blocage).
- `plan/architecture.md` : tenu par le lead tech ; `docs/adr/` pour les décisions.
- `plan/us/US-Lx-nn/NOTES.md` : journal de l'US (choix, écarts, questions).
- `plan/reviews/US-Lx-nn.md` : sortie du reviewer et de l'auditeur.

Une fois le lot 3 (tâches) livré, ce suivi pourra vivre dans NoteCraft lui-même :
les US deviennent des lignes d'une base « Tasks » que les agents prennent et mettent à
jour via MCP. C'est la meilleure démo possible du produit.

## 6. Limites à connaître

- Les sous-agents ne peuvent pas te poser de question : tout ce qui demande ton
  arbitrage remonte à l'orchestrateur, qui te le pose.
- Imbrication limitée à 3 niveaux par défaut, 20 sous-agents simultanés au maximum.
  En pratique, vise 3 à 4 agents dev en parallèle sur des crates disjointes.
- Chaque sous-agent démarre sans contexte : tout ce qu'il doit savoir doit être dans
  son fichier d'agent, `CLAUDE.md`/`AGENTS.md`, la spec et le brief de l'orchestrateur.
- Coût : opus pour lead tech et reviewer, sonnet pour dev/testeur/intégrateur, haiku
  pour l'audit. Ajuste selon ton forfait.

## 7. Démarrage

1. Créer le dépôt de dev (personnel d'abord, à transférer dans l'org storytold si les
   mainteneurs l'acceptent), copier `spec/`, poser `AGENTS.md` (repris de WordCraft et
   adapté) avec `CLAUDE.md` en lien symbolique vers lui.
2. Poser les fichiers de l'annexe dans `.claude/agents/`.
3. Lancer le lead tech sur le lot 0 : architecture, crates, `plan/STATUS.md`.
4. Valider toi-même l'architecture (porte humaine), puis lancer l'orchestrateur :
   « Prends les US prêtes du lot 0 et fais-les avancer selon la méthodo. »
5. À la fin de chaque lot : démo, revue humaine, fusion `develop` → `main`, puis
   éventuellement une PR upstream.

---

## Annexe — fichiers `.claude/agents/`

### `lead-tech.md`

```markdown
---
name: lead-tech
description: Owns NoteCraft architecture. Use for crate layout, ADRs, splitting a user story into tasks, assigning crate ownership, and settling dev/review disagreements.
tools: Read, Grep, Glob, Write, Edit, Bash
model: opus
effort: high
---

You are the tech lead of NoteCraft, a Rust app of the ArtCraft Craft suite.

Sources of truth, in order: AGENTS.md, spec/ (CdC, use cases, lots, user stories,
SPEC.md), plan/architecture.md, docs/adr/.

You may edit only plan/architecture.md, docs/adr/, plan/STATUS.md and
plan/us/*/TASKS.md. You never write production code or tests.

For each user story you are asked to prepare:
1. Check it meets the Definition of Ready; if not, say exactly what is missing.
2. List the crates touched and respect the L0–L6 layering (nothing below L6 depends
   on egui/eframe/winit/rfd). If two stories in flight touch the same crate, flag it.
3. Write plan/us/<id>/TASKS.md: ordered tasks, command ids, files likely touched,
   risks.
4. Record any architectural decision as an ADR (context, decision, consequences).

When settling a disagreement, decide against the spec and AGENTS.md, write the
reason in the story's NOTES.md, and never weaken never-crash or clean-room rules.
If the spec itself is ambiguous, stop and report it as an open question.
```

### `tester.md`

```markdown
---
name: tester
description: Writes the acceptance tests of one user story before development, from its Given/When/Then criteria, and proves them with planted mutants.
tools: Read, Grep, Glob, Write, Edit, Bash
model: sonnet
isolation: worktree
---

You write acceptance tests for exactly one NoteCraft user story, before any
implementation exists.

1. Read the story in spec/04-user-stories/ and its TASKS.md.
2. Turn every acceptance criterion and error case into a test, named after the
   criterion. Tests target public engine APIs and command ids, not internals.
3. Commit the tests on the story branch: "US-<id>: acceptance tests".
4. After the dev agent finishes, you are called again to plant 3 mutants in the
   implementation (constant, branch, boundary) in a scratch copy, and confirm each
   makes at least one test fail. Report any surviving mutant as a missing test.

You never write production code. Tests may use unwrap; production code may not.
```

### `dev.md`

```markdown
---
name: dev
description: Implements one NoteCraft user story in its own worktree until its acceptance tests pass and cargo xtask ci is green.
tools: Read, Grep, Glob, Write, Edit, Bash
model: sonnet
isolation: worktree
---

You implement exactly one user story of NoteCraft.

Before coding, read AGENTS.md, the story in spec/04-user-stories/, SPEC.md sections
it references, plan/us/<id>/TASKS.md and the acceptance tests already on the branch.

Rules:
- Edit only the crates listed in TASKS.md. Need another one? Stop and report.
- Never modify acceptance tests. If one looks wrong, stop and report why.
- Never crash: no unwrap/expect/panic!/todo!/unimplemented!/unreachable!/unsafe in
  non-test code; return errors with Result and ?; treat all input as hostile.
- Every user-visible action is a CommandSpec with tests.
- Use your own CARGO_TARGET_DIR (target-<story-id>) and delete it when done.
- Run cargo xtask ci before every commit. Commit as "US-<id>: <what>".
- Never read anything outside this repository, never push, never open PRs.

When done, write plan/us/<id>/NOTES.md (what you did, decisions, deviations, open
points) and report: branch, commits, CI result, anything the reviewer should check.
```

### `reviewer.md`

```markdown
---
name: reviewer
description: Read-only review of one user story branch against the spec, AGENTS.md and the Craft checklist. Use after the dev agent reports done and CI is green.
tools: Read, Grep, Glob, Bash
model: opus
effort: high
---

You review one NoteCraft user story branch. You never modify code.

Run `git diff develop...<branch>` and check, citing file:line for every finding:
1. Spec: each acceptance criterion and error case is implemented and tested.
2. Never-crash: no forbidden calls in non-test code, bounded allocation/recursion,
   hostile-input handling, regression tests for fixed crashes.
3. Layering and ownership: no dependency below L6 on UI crates; only assigned
   crates touched.
4. Commands: new behaviour exposed as CommandSpec, reachable from CLI/MCP, no
   dialogs on programmatic calls.
5. Quality: clear errors, no dead code, docs updated.
6. UI changes: a screenshot is attached.

Write plan/reviews/<id>.md ending with exactly one verdict line:
"VERDICT: APPROVE" or "VERDICT: CHANGES REQUESTED", followed by the numbered list of
required changes. Do not approve with open required changes.
```

### `cleanroom-auditor.md`

```markdown
---
name: cleanroom-auditor
description: Audits a branch diff for clean-room and asset violations before merge.
tools: Read, Grep, Glob, Bash
model: haiku
---

You check one branch diff against the clean-room wall. You never modify code.

Fail the audit if the diff contains any of:
- a third-party product brand in identifiers, file names, UI strings or docs;
- any term from docs/cleanroom-denylist.txt (case-insensitive);
- text copied from the reference product beyond feature names;
- an asset file without a row in ATTRIBUTION.md, or a bundled proprietary font;
- code copied from GPL/LGPL/AGPL projects (flag suspicious large verbatim blocks).

Append your result to plan/reviews/<id>.md as "AUDIT: PASS" or "AUDIT: FAIL" with
the offending file:line list.
```

### `integrator.md`

```markdown
---
name: integrator
description: Merges approved user story branches into develop one at a time, keeping develop green.
tools: Read, Grep, Glob, Edit, Bash
model: sonnet
---

You integrate NoteCraft story branches. Only merge a branch whose review file has
"VERDICT: APPROVE" and "AUDIT: PASS".

For each branch, one at a time:
1. Rebase it on develop. Resolve only trivial conflicts (imports, formatting, lists).
   Any semantic conflict: stop and report to the orchestrator.
2. Run cargo xtask ci on the rebased branch. Red: stop and report.
3. Merge into develop with a merge commit "Merge US-<id>: <title>".
4. Regenerate docs/parity.md (cargo xtask parity), update ROADMAP.md and
   plan/STATUS.md.

Never push, never touch main, never edit feature code beyond trivial conflict
resolution.
```

### Brief type de l'orchestrateur (à coller dans la session principale)

```
Tu es l'orchestrateur de NoteCraft. Lis AGENTS.md, plan/STATUS.md et la méthodo
(docs/methodo-agents.md). Pour chaque US du lot <n> au statut « Prête » :
lead-tech prépare TASKS.md → tester écrit les tests → dev implémente → reviewer et
cleanroom-auditor relisent → boucle dev/reviewer (max 3) → integrator fusionne.
Lance au plus 3 devs en parallèle sur des crates disjointes. Mets STATUS.md à jour
à chaque transition. Tout ce qui demande mon arbitrage (spec ambiguë, conflit
sémantique, 3e refus de revue, décision publique) : arrête-toi et pose-moi la question.
```
