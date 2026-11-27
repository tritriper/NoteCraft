# ADR 0006 — Multi-file commits: an undo journal in `.cache/journal/`, recovery on open

- **Status:** accepted (US-L0-10)
- **Date:** 2026-11-27
- **Story:** US-L0-10, atomic multi-file transactions and crash recovery (F-CMD-10; ENF-03,
  ENF-12)

## Context

ADR 0003 gave the session one commit path. A command stages its changes, the session writes them
in order, and a failed write restores the files already written, in reverse, from memory. ADR
0005 connected that path to the workspace folder, and every file is written with
`craft_atomic::atomic_write`. One file is therefore always either old or new (ENF-12). A
command that changes several files is not crash-safe yet. If the process dies between its second
and third write, the in-memory rollback never runs, and the workspace stays half-written. L1's
`page.move` moves a file, its folder and every link to it "in one transaction" (file-format
§2.0), so this is the normal case for many commands, not a corner case.

US-L0-10 asks for two things. A crash in the middle of a many-file command must leave the
workspace entirely before or entirely after it once it reopens (AT-L0-10-a). A command that
fails must leave the hash of the workspace tree unchanged (AT-L0-10-b). The spec does not say
where a journal lives or how it is written. file-format §2 gives the workspace a `.gitignore`
that holds `.cache/`, and §9 describes `.cache/` as rebuildable. craft-libs (rev `110d4a7`) has
no journal or transaction crate. craft-atomic makes one file atomic and leaves its temporary
files behind after a crash: "sweeping orphaned staging files is the responsibility of the
consuming application".

## Decisions

| # | Decision | Reason |
|---|---|---|
| 1 | **An undo journal (rollback log), not a redo log.** Before the first target write of a commit, the session makes the *before* state of every file it will change durable. After a crash, recovery always **rolls back** to before. It never rolls forward. | A redo log would also need the before bytes, so that a failed command can still be undone without anything being re-applied on the next open. An undo log is one mechanism for both a failure and a crash, and the "before" outcome is what a failed or interrupted command must give (ENF-03). |
| 2 | **Only commits that change two or more files are journaled.** A commit of one change stays a single `atomic_write` or remove, which is atomic already. The engine plans first: it reads every staged path, drops changes whose bytes are equal, and journals only when two or more changes remain. | Most commands (typing, a property edit) change one file. Journaling them would add four synchronous flushes per keystroke for no safety gain. |
| 3 | **The journal lives in the workspace at `.cache/journal/`**: `head.json` plus one blob per file that existed, `<k>.before` (k is the entry's index). `.cache/` is already in the `.gitignore` that `workspace.create` writes, so a journal left by a crash is never committed, and the file-format layout does not change. | It must sit on the same volume as the files it protects, and it must be found by whichever process opens the workspace next. A folder outside the workspace (app data) would not be. The cost is that `.cache/` now holds one thing that is not rebuildable: deleting `.cache/journal/` after a crash gives up the rollback (the workspace stays as the crash left it, which is no worse than without a journal). This is an open point for the spec room (file-format §9). |
| 4 | **The protocol**, through the engine's `Files` port: (1) **plan**, which only reads; (2) write each blob, then `head.json`. Each one is an `atomic_write`, so each is flushed before the next starts. Writing `head.json` is the *armed* point; (3) apply the target writes and removes in order; (4) **remove `head.json`. This is the commit point**: success is returned only after it; (5) remove the blobs (best effort). A failure in (2) removes what was written and touches no target. A failure or panic in (3) or (4) rolls back from memory as before (ADR 0003 #4), then clears the journal. If the rollback or the clear fails, the journal stays and the session marks recovery as pending (#7). | Each step is atomic, and the order makes every crash point recoverable. Before the armed point, no target was touched. Between armed and commit, `head.json` lists everything needed to go back. After commit, the change is done. |
| 5 | **`head.json` format.** JSON in the canonical form of file-format §8, tagged with a craft-format family: `{"format": "notecraft-journal1", "command": "<id>", "entries": [{"path": "<RelPath>", "before": "<sha256 hex>" \| null, "after": "<sha256 hex>" \| null}]}`. The blob `<k>.before` exists exactly when entry k has a `before`. Bounds: the file is at most 4 MiB (the store's read cap), there are at most 100,000 entries, a path is a valid `RelPath` outside `.cache/`, paths are unique, a hash is 64 lower-case hex characters, and an entry may not have `before` and `after` both null. A commit that would exceed a bound fails with `io` (`details.reason: "too_large"`) before anything is written. The digest is SHA-256 (`sha2`, already in `Cargo.lock` through craft-format; MIT OR Apache-2.0), exposed as `notecraft_store::digest`. | It is one small file to read on open. The blobs keep large bytes out of JSON, and each blob stays under the read cap because it is a copy of a file that was read through the same cap. The *after* digest lets recovery tell "we wrote this" from "someone edited it since" without storing the new bytes twice. The craft-format tag lets an older build recognise a journal from a newer one and leave it alone. |
| 6 | **Recovery on `workspace.open`, all or nothing.** It runs only when the opened handle may write: not read-only (requested or `newer_format`) and inside the write root. With no `head.json`, nothing happens and nothing is written, so AT-L0-03-d still holds. Otherwise the engine parses the head and classifies every entry by its current bytes: **before** (nothing to do), **after** (restore) or **other** (changed outside NoteCraft since the crash). It reads and checks every blob it needs against its `before` digest. Only then does it write. It restores the "after" entries in reverse order, then clears the journal. If the head is corrupt or from a newer format, an entry is "other", a blob is missing or does not match, or a write fails, **nothing more is written and the journal is kept**. The workspace then opens **read-only** with `readOnlyReason: "recovery"`, and the open result says why. | Like undo (ADR 0003 #5), recovery never overwrites a change made outside NoteCraft and never restores half of a command. A read-only open is safe, it reports the problem (rule 15), and the next open retries. Recovery is idempotent: it is decided per entry from the current bytes. |
| 7 | **Recovery inside a session.** When a rollback or a journal clear fails, the session sets a pending flag. The next commit (undo and redo included) first runs the same recovery through its `Files`. If that recovery is refused, the command fails with `io` (`details.reason: "recovery_pending"`), and nothing else is written. | A session that keeps working over a pending journal could write over a file the journal still has to restore. |
| 8 | **`.cache/` is reserved.** A staged write or remove whose first path segment is `.cache` (ASCII, any case) fails the commit with `io` (`details.path`, `details.reason: "reserved"`) before anything is read or written. The journal's own writes do not go through staging, so they are not affected. | A command must not be able to forge or delete a journal, and the cache is not undoable content. Case folding covers case-insensitive volumes (macOS, Windows). |
| 9 | **A port opts in: `Files::journaled(&self) -> bool`, default `false`.** `DiskFiles` returns `true`. A port that keeps the default is still rolled back in memory, as before, but it is not crash-safe. The US-L0-02 test fixtures keep the default, so their write counts and their meaning do not change. US-L0-10's crash fixtures return `true`. | Only the port knows whether `.cache/journal/` on it survives a crash. Because the journal goes through the same port, a test port can crash the protocol at every step without any test-only code in the library (ADR 0003 #9). |
| 10 | **Sweep on open.** When the handle may write, after recovery, the store removes leftovers in one walk. That walk does not follow symlinks, skips `.git`, and is bounded to depth 64 and 1,000,000 entries. It removes regular files named like craft-atomic temporaries (`.<name>.<pid>-<n>.craft_tmp`), every file in `.cache/journal/` when there is no `head.json`, and `.notecraft-new.*.craft_tmp` staging folders (real folders, not links) in the project folder. `workspace.create` removes leftover staging folders before it stages its own. It is best effort: a failure is ignored, because a leftover is never read as content. | These are the remains of a single-file write or a `create` killed half-way (ADR 0005 #4 and consequences). One pattern, one place. |
| 11 | **Durability of the commit point.** Removing `head.json` goes through the store's `remove`, which then flushes the folder (`craft_atomic::RealFs` `sync_dir`, best effort). | Without the flush, a power cut right after success could bring `head.json` back, and the next open would roll back a command that succeeded. That is still "entirely before", but it is a surprise. The flush narrows the window to what the OS guarantees. |

## Consequences

- A command that changes `n ≥ 2` files costs about `3n + 2` file operations instead of `n`: the
  blobs, the head, the targets, the head removal and the blob removals. Memory does not change,
  because the before bytes were already kept for undo. A one-file command costs what it did.
- **"Entirely before or entirely after"** has an exact meaning. A crash before the commit point
  gives *before* after reopening. A crash after it gives *after*. A failed command gives
  *before* as soon as the disk accepts the rollback. If the disk refuses even the rollback, the
  journal stays, and the next commit or the next open restores *before*.
- The `workspace.open` result gains `recovery`: `null`, or an object with `state`
  (`rolled_back`, `pending` or `refused`), `command`, and for `refused` also `reason`
  (`conflict`, `corrupt`, `newer` or `io`), `paths` and `message`. `ReadOnlyReason` gains
  `Recovery` (`"recovery"`). These are additions to commands.md §2 (an open point for the spec
  room).
- The store gains `sha2` as a direct dependency. It adds no new crate to `Cargo.lock` and builds
  for wasm. On wasm, `std::fs` fails at run time, so the journal, recovery and sweep report `io`
  or do nothing, like every other store write (ADR 0005).
- **Two processes.** Until US-L0-11 (the workspace lock), a second process that opens the
  workspace could roll back a commit that the first process is still running, and its sweep
  could delete a live temporary file. US-L0-11 must take the lock before recovery and the sweep,
  and must treat a stale lock as a crashed writer whose journal it then recovers. No second
  process exists yet: the CLI and MCP are US-L0-05 and US-L0-06.
- An empty folder created by a failed or rolled-back write (a new parent folder) is not removed.
  Git ignores it, and so does the tree hash of AT-L0-10-b. L1, where a folder is part of the page
  tree (file-format §2.0), must prune the folders a rollback or recovery created.
- L1's cache rebuild must not delete `.cache/journal/`, and recovery must run before the cache is
  loaded or rebuilt.
- The `batch` command of MCP (commands.md §1, "one step") must commit through one journal.
- The symlink check-then-use window of ADR 0005 is not narrowed here; that is left to US-L0-11
  (one writer) and to a later craft-libs API that opens files relative to a folder handle.
