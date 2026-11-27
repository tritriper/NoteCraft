//! The workspace write lock (ADR 0007): an OS advisory lock on `.cache/lock` (std `File::try_lock`)
//! and an advisory holder record in `.cache/lock.json`.

use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
#[cfg(target_arch = "wasm32")]
use std::time::Duration;

use serde_json::{Value, json};

use crate::error::clip;
use crate::workspace::Workspace;

/// The locked file, inside the workspace. It is only locked, never read or written.
pub const LOCK_FILE: &str = ".cache/lock";
/// The holder record. Advisory: used in messages and to tell that a holder did not release.
pub const LOCK_RECORD: &str = ".cache/lock.json";
/// A record larger than this is not read (it is treated as unreadable).
pub const LOCK_RECORD_MAX_BYTES: u64 = 4096;

const COMMAND_MAX_BYTES: usize = 200;
const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// Who holds (or held) the write lock. Advisory only (ADR 0007 #5).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Holder {
    pub pid: Option<u32>,
    pub command: Option<String>,
    pub started: Option<u64>,
}

fn clip_bytes(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s.get(..end).unwrap_or_default()
}

impl Holder {
    /// Tolerant and total: garbage gives `Holder::default()`; a key of the wrong type is `None`;
    /// `command` is clipped to 200 bytes on a character boundary. Unknown keys are ignored.
    pub fn parse(bytes: &[u8]) -> Holder {
        let bytes = bytes.strip_prefix(&BOM[..]).unwrap_or(bytes);
        let Ok(text) = std::str::from_utf8(bytes) else { return Holder::default() };
        let Ok(Value::Object(map)) = serde_json::from_str::<Value>(text) else { return Holder::default() };
        Holder {
            pid: map.get("pid").and_then(Value::as_u64).and_then(|p| u32::try_from(p).ok()),
            command: map.get("command").and_then(Value::as_str).map(|c| clip_bytes(c, COMMAND_MAX_BYTES).to_string()),
            started: map.get("started").and_then(Value::as_u64),
        }
    }

    /// `{"pid": …, "command": …, "started": …}` + "\n", keys in that order, `null` for `None`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let command = self.command.as_deref().map(|c| clip_bytes(c, COMMAND_MAX_BYTES));
        let mut out = json!({ "pid": self.pid, "command": command, "started": self.started }).to_string().into_bytes();
        out.push(b'\n');
        out
    }
}

/// Why the lock was not taken.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LockError {
    /// Another live process holds it after the wait. `holder`: its record, if there is one.
    Held { holder: Option<Holder> },
    /// The file system (or the target, wasm) has no file locks.
    Unsupported,
    /// The handle is read-only.
    ReadOnly,
    /// `.cache/` or a lock path is unusable. Nothing was changed.
    Io { path: String, message: String },
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LockError::Held { holder: Some(Holder { pid: Some(pid), .. }) } => write!(f, "the workspace is locked by process {pid}"),
            LockError::Held { .. } => f.write_str("the workspace is locked by another process"),
            LockError::Unsupported => f.write_str("this file system does not support file locks"),
            LockError::ReadOnly => f.write_str("the workspace is read-only"),
            LockError::Io { path, message } => write!(f, "{}: {}", clip(path), clip(message)),
        }
    }
}

impl std::error::Error for LockError {}

/// The held write lock. Not `Clone`. Dropping it releases it (ADR 0007 #4), best effort.
#[derive(Debug)]
pub struct WriteLock {
    file: Option<fs::File>,
    record_path: PathBuf,
    wrote_record: bool,
    stale: Option<Holder>,
}

impl WriteLock {
    /// The record left by a holder that died without releasing (ADR 0007 #6), read when this
    /// lock was taken. `Some(Holder::default())` when that record was unreadable.
    pub fn stale(&self) -> Option<&Holder> {
        self.stale.as_ref()
    }
}

impl Drop for WriteLock {
    fn drop(&mut self) {
        // The record goes first, then the lock is released. The lock file itself is persistent
        // (ADR 0007 #4): it is never unlinked, so no waiter can ever hold a deleted file.
        if self.wrote_record {
            let _ = fs::remove_file(&self.record_path);
        }
        if let Some(f) = self.file.take() {
            let _ = f.unlock();
        }
    }
}

/// The record, read without the lock; `None` unless `.cache` is a real folder and the record a
/// regular file.
pub(crate) fn read_record(ws: &Workspace) -> Option<Holder> {
    let cache = ws.dir().join(".cache");
    if !fs::symlink_metadata(&cache).ok()?.file_type().is_dir() {
        return None;
    }
    read_regular(&ws.dir().join(LOCK_RECORD))
}

fn read_regular(path: &Path) -> Option<Holder> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.file_type().is_file() {
        return None;
    }
    if meta.len() > LOCK_RECORD_MAX_BYTES {
        return Some(Holder::default());
    }
    let mut bytes = Vec::new();
    let read = fs::File::open(path).and_then(|f| f.take(LOCK_RECORD_MAX_BYTES + 1).read_to_end(&mut bytes));
    if read.is_err() || bytes.len() as u64 > LOCK_RECORD_MAX_BYTES {
        return Some(Holder::default());
    }
    Some(Holder::parse(&bytes))
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn acquire(ws: &Workspace, _me: &Holder, _wait: Duration) -> Result<WriteLock, LockError> {
    // No timer and no sleep on this target (they panic on wasm32-unknown-unknown).
    if ws.read_only().is_some() {
        return Err(LockError::ReadOnly);
    }
    Err(LockError::Unsupported)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::acquire;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::fs::{self, File, OpenOptions, TryLockError};
    use std::io::{self, Write};
    use std::path::Path;
    use std::time::{Duration, Instant};

    use super::{Holder, LOCK_FILE, LOCK_RECORD, LockError, WriteLock, read_record, read_regular};
    use crate::workspace::Workspace;

    /// Each retry needs another process to have released the lock, so this only stops a runaway.
    const MAX_IDENTITY_RETRIES: u32 = 100_000;
    /// After this many retries, a wait that has run out gives `Held` instead of trying again.
    const SOFT_RETRIES: u32 = 16;
    const MAX_WAIT: Duration = Duration::from_secs(3600);

    /// An `io` error that names the step (`open`, `lock`, `identity`) and carries the OS code.
    fn step_err(path: &Path, step: &str, e: &io::Error) -> LockError {
        let code = e.raw_os_error().map_or(String::new(), |c| format!(" (os error {c})"));
        LockError::Io { path: path.to_string_lossy().into_owned(), message: format!("{step}: {e}{code}") }
    }

    fn refuse(path: &Path, message: &str) -> LockError {
        LockError::Io { path: path.to_string_lossy().into_owned(), message: message.to_string() }
    }

    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd", target_os = "dragonfly"))]
    const ENOLCK: i32 = 77;
    #[cfg(all(
        unix,
        not(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "dragonfly"
        ))
    ))]
    const ENOLCK: i32 = 37;

    /// Whether this error means that the file system has no locks.
    fn no_locks(e: &io::Error) -> bool {
        e.kind() == io::ErrorKind::Unsupported || is_enolck(e)
    }

    #[cfg(unix)]
    fn is_enolck(e: &io::Error) -> bool {
        e.raw_os_error() == Some(ENOLCK)
    }

    #[cfg(not(unix))]
    fn is_enolck(_e: &io::Error) -> bool {
        false
    }

    /// Makes sure `.cache` is a real folder. It is never removed, so a first writer cannot race a
    /// removal.
    fn ensure_cache(ws: &Workspace) -> Result<(), LockError> {
        let cache = ws.dir().join(".cache");
        // `ws.dir()` is canonical and `.cache` is its direct child: a link there is refused below,
        // by `symlink_metadata`, and nothing is resolved twice, so a `.cache` that another writer
        // creates meanwhile is never mistaken for a dangling link.
        match fs::symlink_metadata(&cache) {
            Ok(m) if m.file_type().is_dir() => Ok(()),
            Ok(_) => Err(refuse(&cache, "not a folder")),
            Err(e) if e.kind() == io::ErrorKind::NotFound => match fs::create_dir(&cache) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
                Err(e) => Err(step_err(&cache, "open", &e)),
            },
            Err(e) => Err(step_err(&cache, "open", &e)),
        }
    }

    enum Opened {
        File(File),
        Retry,
        /// Windows: a third program holds the file without sharing (error 32). Retried until the
        /// deadline, then `io`.
        #[cfg(windows)]
        Busy(io::Error),
    }

    fn open_error(path: &Path, e: io::Error) -> Result<Opened, LockError> {
        if e.kind() == io::ErrorKind::NotFound || e.kind() == io::ErrorKind::AlreadyExists {
            return Ok(Opened::Retry);
        }
        #[cfg(windows)]
        if e.raw_os_error() == Some(32) {
            return Ok(Opened::Busy(e));
        }
        Err(step_err(path, "open", &e))
    }

    fn open_lock_file(path: &Path) -> Result<Opened, LockError> {
        let mut opts = OpenOptions::new();
        opts.read(true).write(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            opts.share_mode(1 | 2);
        }
        match fs::symlink_metadata(path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                opts.create_new(true);
                match opts.open(path) {
                    Ok(f) => Ok(Opened::File(f)),
                    Err(e) => open_error(path, e),
                }
            }
            Err(e) => Err(step_err(path, "open", &e)),
            Ok(m) if m.file_type().is_file() => match opts.open(path) {
                Ok(f) => Ok(Opened::File(f)),
                Err(e) => open_error(path, e),
            },
            Ok(_) => Err(refuse(path, "open: not a regular file")),
        }
    }

    #[cfg(unix)]
    fn same_file(file: &File, path: &Path) -> bool {
        use std::os::unix::fs::MetadataExt;
        match (file.metadata(), fs::symlink_metadata(path)) {
            (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
            _ => false,
        }
    }

    #[cfg(not(unix))]
    fn same_file(_file: &File, _path: &Path) -> bool {
        true
    }

    enum Tried {
        Locked,
        Held,
        Unsupported,
        Failed(io::Error),
    }

    fn try_once(file: &File) -> Tried {
        match file.try_lock() {
            Ok(()) => Tried::Locked,
            Err(TryLockError::WouldBlock) => Tried::Held,
            Err(TryLockError::Error(e)) if no_locks(&e) => Tried::Unsupported,
            Err(TryLockError::Error(e)) => Tried::Failed(e),
        }
    }

    pub(crate) fn acquire(ws: &Workspace, me: &Holder, wait: Duration) -> Result<WriteLock, LockError> {
        if ws.read_only().is_some() {
            return Err(LockError::ReadOnly);
        }
        let lock_path = ws.dir().join(LOCK_FILE);
        let record_path = ws.dir().join(LOCK_RECORD);
        let deadline = Instant::now().checked_add(wait.min(MAX_WAIT));
        let mut attempts = 0u32;
        #[cfg(windows)]
        let mut busy: Option<io::Error> = None;
        let file = 'outer: loop {
            attempts += 1;
            if attempts > SOFT_RETRIES && deadline.is_some_and(|d| Instant::now() >= d) {
                #[cfg(windows)]
                if let Some(e) = &busy {
                    return Err(step_err(&lock_path, "lock", e));
                }
                return Err(LockError::Held { holder: read_record(ws) });
            }
            if attempts > MAX_IDENTITY_RETRIES {
                return Err(refuse(&lock_path, "identity: the lock file keeps changing"));
            }
            ensure_cache(ws)?;
            let file = match open_lock_file(&lock_path)? {
                Opened::File(f) => f,
                Opened::Retry => {
                    // Another process is making the file, or it was removed by hand.
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                }
                #[cfg(windows)]
                Opened::Busy(e) => {
                    busy = Some(e);
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                }
            };
            let mut pause = Duration::from_millis(10);
            loop {
                match try_once(&file) {
                    Tried::Locked => {
                        if same_file(&file, &lock_path) {
                            break 'outer file;
                        }
                        continue 'outer;
                    }
                    Tried::Held => {
                        let left = deadline.map(|d| d.saturating_duration_since(Instant::now()));
                        if left == Some(Duration::ZERO) {
                            return Err(LockError::Held { holder: read_record(ws) });
                        }
                        std::thread::sleep(left.map_or(pause, |l| pause.min(l)));
                        pause = (pause * 2).min(Duration::from_millis(100));
                    }
                    Tried::Unsupported => return Err(LockError::Unsupported),
                    Tried::Failed(e) => {
                        // The path no longer names this file (removed by hand): start again.
                        if !same_file(&file, &lock_path) {
                            continue 'outer;
                        }
                        return Err(step_err(&lock_path, "lock", &e));
                    }
                }
            }
        };
        // Under the lock: a record that is already here belongs to a holder that died.
        let stale = read_regular(&record_path);
        let wrote_record = match fs::symlink_metadata(&record_path) {
            Ok(m) if m.file_type().is_dir() => false,
            Ok(_) => {
                let _ = fs::remove_file(&record_path);
                write_record(&record_path, me)
            }
            Err(_) => write_record(&record_path, me),
        };
        Ok(WriteLock { file: Some(file), record_path, wrote_record, stale })
    }

    fn write_record(path: &Path, me: &Holder) -> bool {
        match OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(mut f) => {
                let _ = f.write_all(&me.to_bytes());
                true
            }
            Err(_) => false,
        }
    }
}
