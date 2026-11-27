# ADR 0007 — One writer at a time: an OS lock per command in `.cache/lock`, stale holders recovered

- **Status:** proposed (US-L0-11)
- **Date:** 2026-11-27
- **Story:** US-L0-11, workspace lock and single engine (F-CMD-11; ENF-03)

## Context

The app, the CLI (`notecraft run`, US-L0-05) and the MCP server (US-L0-06) each run their own
engine session, in their own process, over the same workspace folder. Every surface uses the
same engine code and the same commands (EF-12), but nothing stops two processes from writing at
once. Two read-change-write commands can then lose an update. A second process can also roll back
a live multi-file commit or sweep away a live temporary file: ADR 0006 (consequences) asks this
story to take a lock before recovery and the sweep, and to treat a stale lock as a crashed writer.

The story asks that the writes of the app and of a CLI command be serialised ("the CLI goes
through the app's engine or waits") and that none overwrites the other (AT-L0-11-a). A stale lock
left by a crashed process must be detected and cleared on open (AT-L0-11-b). When the lock cannot
be taken, the error is `disabled` with reason `locked`. The spec also says that the app and an MCP
server work on the same workspace at once: the server writes files and the app reloads
(SPEC §10.3; AT-L0-06-c). A bridge from MCP into the app's session is refused for now. A lock held
by the app for as long as a workspace is open would contradict that. The spec does not say where
the lock lives or how it is taken.

The suite already locks files with the standard library: craft-libs `110d4a7` uses
`std::fs::File::try_lock` in `suite-account`, `settings`, `platform` (autosave), `ipc` (instance
records) and `theme-store`. Each is a private helper. None is a crate NoteCraft could depend on
below the UI, because `platform` brings the theme, i18n and clipboard. The workspace forbids
`unsafe`, so calling `flock` or `LockFileEx` by hand is not possible. `rust-version` is 1.95, and
`File::lock` and `try_lock` are stable since 1.89.

## Decisions

| # | Decision | Reason |
|---|---|---|
| 1 | **The lock is a write lock taken for each command, not for each session.** A process holds it while one mutating command runs: from just after the `enabled` check, through the handler, the commit and the undo record, then it releases it. `edit.undo` and `edit.redo` take it too. A read-write `workspace.open` holds it during recovery and the sweep. Read commands never take it. Another writer **waits** for it, up to a bounded time. "Single engine" means one engine for every surface (EF-12), not one process. Routing a command into the running app's session is the control channel (US-L0-09, `engine.execute`); it is not needed to serialise writes. | AT-L0-11-a allows "waits". A lock held for the whole session would refuse MCP while the app is open, against SPEC §10.3 and AT-L0-06-c. The handler runs under the lock, so a read-change-write is serialised, not only the file writes. |
| 2 | **Mechanism: an OS advisory lock through `std::fs::File::try_lock`.** On Unix, std uses `flock`; on Windows, `LockFileEx`. No new dependency and no `unsafe`. It is **not** a pid file. The OS drops the lock when the holding process dies, however it dies, so whether a holder is alive is decided by the OS, never by a pid. | A pid file needs a liveness check that std does not offer (`kill(pid, 0)`, `OpenProcess`), it is wrong after pid reuse, and it says nothing about a process on another host. This is the suite's mechanism already (rule 9; see Context). |
| 3 | **Where: `notecraft/.cache/lock` (the locked file, always empty) and `notecraft/.cache/lock.json` (the holder record).** The existing `.gitignore` already covers `.cache/`, so the lock never shows in a Git diff. The record is a separate file because a `LockFileEx` lock blocks other processes from reading the locked file on Windows. The lock file is never read or written: it is only locked. | Same volume and folder as the files it protects, found by every process that opens the workspace, never committed. The lock and the journal (ADR 0006 #3) are the two non-rebuildable things in `.cache/` (spec-room point). |
| 4 | **A released lock leaves nothing behind.** Acquire: check `.cache/` with the store's confinement, create it if absent (and remember that this acquisition created it), then open `.cache/lock`: `create_new` when it is absent (which never follows a link), otherwise open it only if `symlink_metadata` says it is a regular file. Then `try_lock`. On Unix, after `try_lock` succeeds, the path must still name the locked file (same `dev` and `ino`), or the handle is dropped and the acquisition starts again (at most 16 times, then `io`). On Windows, the file is opened without `FILE_SHARE_DELETE` (`OpenOptionsExt::share_mode`), so nobody can delete it while a handle is open. Then the record is written. Release, best effort: remove the record; on Unix, unlink the lock file and then close it; on Windows, close it and then delete it (this fails harmlessly when another process has it open, and that process then owns the file); then remove `.cache/` if this acquisition created it and it is empty. | A clean open and a failed command leave the tree byte-identical (AT-L0-03-d, AT-L0-10-b), and a crash leaves visible traces. With the identity check, deleting the lock file cannot let two processes hold "the" lock: a waiter that wakes on an unlinked file starts again. |
| 5 | **The holder record** is `{"pid": <u32>, "command": "<id>", "started": <ms since 1970>}` with a final newline. `started` comes from `model::SystemClock` and is `null` when the clock fails. There is no host name, user name or path in it (rule 20). It is written after the lock is taken: an existing entry at that path that is a file or a link is removed (a link is removed, never followed), then the record is written with `create_new`, without a flush. Reading is tolerant: only a regular file, at most 4 KiB, any JSON object. A missing or unusable key is `null`, and unknown keys are ignored. Unreadable bytes give an unknown holder, never an error. The record is **advisory**. It is used only in messages and to tell that the previous holder did not release the lock. | Shown to people ("process 4182 is running `page.move`"). No flush, because a lost record costs only a less precise message (the journal head, which is flushed, still drives recovery), and two flushes per keystroke would not be worth it. |
| 6 | **Stale lock = a record that is present when the lock is acquired.** A live holder always holds the OS lock while its record exists. So if a process gets the lock and finds a record, the previous holder died without releasing it. The pid in the record is never checked, so pid reuse cannot fool it. Clearing a stale lock means: overwrite the record with ours, run journal recovery (ADR 0006 #6), sweep (ADR 0006 #10), all under the lock, then release as in #4. `workspace.open` reports it as `staleLock: {pid, command, started}` (each may be `null`), or `null` when there was none (rule 15). A mutating command that finds a stale lock clears it in the same way, without reporting it in its result. | AT-L0-11-b: detected and cleared, on every platform, with no timer and no pid check. The order "lock, then recover, then sweep" closes the two-process gap that ADR 0006 left open. |
| 7 | **Recovery only under the lock.** In a session that has an open workspace, every mutating command, once it holds the lock, runs `recover` if `.cache/journal/head.json` exists, whatever the pending flag says. Under the lock, a head always belongs to a writer that did not finish: this process (the pending flag of ADR 0006 #7) or a crashed one. A refusal stays `io` with `details.reason: "recovery_pending"`, and nothing else is written. A read-write `workspace.open` runs recovery and the sweep only while it holds the lock. Sessions with no workspace (`Session::with_files`, the test ports) take no lock and keep ADR 0006's behaviour. | A process must never roll back or sweep another live writer's files. Checking for a head costs one `metadata` call. |
| 8 | **Bounded wait.** `Session::set_lock_wait(Duration)`, default `LOCK_WAIT` = 5 s (the suite's precedent). Acquisition calls `try_lock` again with a growing pause (10 ms up to 100 ms) until the deadline. `Duration::ZERO` means a single try. NoteCraft never calls the blocking `File::lock`. On a timeout, the command (or the open) fails with `disabled`, `details.reason: "locked"`, `details.cause: "held"` and `details.holder: {pid, command, started} \| null`, and nothing is written. A failed open keeps the previous workspace (ADR 0005 #3). | The error named by the story. A hung or stopped holder cannot freeze another process for ever. The wait is a parameter, so tests and later surfaces (the UI, a CLI flag) choose their own. |
| 9 | **No locks available.** When `try_lock` reports that the file system has no locks (`ErrorKind::Unsupported`, or `ENOLCK` on Unix), the lock "cannot be taken". The command or open gives `disabled`, `reason: "locked"`, `cause: "unsupported"`, `holder: null`, and writes nothing. Reads and `readOnly: true` opens still work. On wasm32 the store returns `Unsupported` at once: it never calls `Instant::now` or `thread::sleep`, which panic on wasm32-unknown-unknown. | Writing without serialisation would be partial behaviour shown as success (rule 15). It is the story's error, and reversible: the spec room may prefer an unlocked fallback with a warning (open point). |
| 10 | **Failures of the lock itself.** A `.cache/` that is not a real folder, or a `.cache/lock` that is not a regular file (a link, even one that stays inside the workspace, a FIFO, a device, a folder named `lock`), a path that leads out of the workspace, a permission error, or too many identity retries all give `io`, with `details.path` and `details.reason: "lock"`. Nothing in the workspace is changed: the entry is not removed, because it is not ours. A record path that is a folder is left alone: the record is not written, and the command goes on. | Never crash, never follow a planted link, never delete what we did not create. A `.cache` link into `pages/` would otherwise let the record logic remove a user's `lock.json`. The lock file is never read, so a huge lock file costs nothing. |
| 11 | **Order in `Session::run` for a mutating command:** validate, then the write gate (`read_only`, `outside_roots`), then `enabled`, then **lock**, then recovery if a head exists, then the handler, the commit and the undo record, then release (an RAII guard: it is also released on an error or a caught panic). **Order in `workspace.open`:** locate, roots, `Workspace::open` (reads only), then, only if the handle may write (not read-only and inside the write root): lock, recovery, sweep, release. A read-only or unwritable open never takes the lock and writes nothing, as before. | No lock is taken for a command that would be refused anyway. A read-only open stays free of side effects on any medium. |
| 12 | **Catalog:** row F-CMD-11 names `workspace.open`, as in catalog.md. | ADR 0005 kept it empty until the lock existed. |

## Consequences

- A mutating command does about six more file-system calls (create and remove the lock file and
  the record, `try_lock`, one identity `stat`, one head `stat`), none of them a flush. A read-write
  open now creates and removes those files too. The tree stays byte-identical, so AT-L0-03-d still
  holds, but such an open fails on a read-only medium (`io`) or a file system without locks
  (`disabled`/`locked`). Opening with `readOnly: true` then works.
- `workspace.open` gains `staleLock` (an addition to commands.md §2, open point for the spec room).
  `disabled` gains `details.cause` and `details.holder`.
- The lock serialises writes, but it does not refresh what a session holds in memory. In L1, a
  handler that edits from cached state must check, under the lock, that the files are still what
  it read (`expectedRev`, file stamps, rescan). Undo already checks the bytes (ADR 0003).
- MCP `batch` (US-L0-06) must hold the lock once for the whole batch, so that a batch is one step
  and is not interleaved with another writer.
- The app (US-L0-07) must not block its UI thread for 5 s: it runs commands off the UI thread, or
  uses a short wait and shows the `locked` error.
- L1's cache rebuild must not delete `.cache/lock` or `.cache/lock.json`, any more than
  `.cache/journal/`.
- `flock` on network file systems differs from one mount to another. On some NFS mounts it is local
  to the client, so two machines writing one share are not serialised. Git (two clones) is the
  supported way to share a workspace between machines (P2). This goes in the user docs with L1.
- A file-system lock does not stop a program that ignores it (an editor, `git checkout`). External
  changes stay the business of the L1 rescan and of undo's byte checks.
- Before the lock existed, the sweep could race a live writer (ADR 0006, consequences). That gap is
  closed. `workspace.create`'s removal of staging folders in the project folder is still not
  locked: two creates in one project at the same instant could disturb each other. That is rare,
  and the result is a failed create, not damage.
- craft-libs already has five private copies of the `try_lock` pattern. A shared folder-lock crate
  (lock, holder record, stale detection) would be a craft-libs PR of its own, once a second app
  needs this one (rule 9).
