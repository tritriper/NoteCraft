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
    lock_path: PathBuf,
    record_path: PathBuf,
    cache_path: PathBuf,
    wrote_record: bool,
    created_cache: bool,
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
        if self.wrote_record {
            let _ = fs::remove_file(&self.record_path);
        }
        let file = self.file.take();
        #[cfg(not(windows))]
        {
            // Unlink before unlock: a waiter that wakes on the unlinked file starts again.
            let _ = fs::remove_file(&self.lock_path);
            drop(file);
        }
        #[cfg(windows)]
        {
            if let Some(f) = file {
                let _ = f.unlock();
                drop(f);
            }
            let _ = fs::remove_file(&self.lock_path);
        }
        if self.created_cache {
            let _ = fs::remove_dir(&self.cache_path);
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
    use crate::path::RelPath;
    use crate::workspace::Workspace;

    const MAX_IDENTITY_RETRIES: u32 = 16;
    const MAX_WAIT: Duration = Duration::from_secs(3600);

    fn io_err(path: &Path, e: impl ToString) -> LockError {
        LockError::Io { path: path.to_string_lossy().into_owned(), message: e.to_string() }
    }

    fn refuse(path: &Path, message: &str) -> LockError {
        LockError::Io { path: path.to_string_lossy().into_owned(), message: message.to_string() }
    }

    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd", target_os = "dragonfly"))]
    const ENOLCK: i32 = 77;
    #[cfg(all(
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd", target_os = "dragonfly"))
    ))]
    const ENOLCK: i32 = 37;

    /// Whether this error means that the file system has no locks.
    fn no_locks(e: &io::Error) -> bool {
        if e.kind() == io::ErrorKind::Unsupported {
            return true;
        }
        #[cfg(unix)]
        if e.raw_os_error() == Some(ENOLCK) {
            return true;
        }
        false
    }

    /// Makes sure `.cache` is a real folder; returns whether this call created it.
    fn ensure_cache(ws: &Workspace) -> Result<bool, LockError> {
        let cache = ws.dir().join(".cache");
        let rel = RelPath::parse(".cache").map_err(|e| io_err(&cache, format!("{e:?}")))?;
        ws.confined(&rel).map_err(|e| io_err(&cache, e))?;
        match fs::symlink_metadata(&cache) {
            Ok(m) if m.file_type().is_dir() => Ok(false),
            Ok(_) => Err(refuse(&cache, "not a folder")),
            Err(e) if e.kind() == io::ErrorKind::NotFound => match fs::create_dir(&cache) {
                Ok(()) => Ok(true),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(false),
                Err(e) => Err(io_err(&cache, e)),
            },
            Err(e) => Err(io_err(&cache, e)),
        }
    }

    enum Opened {
        File(File, bool),
        Retry,
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
                    Ok(f) => Ok(Opened::File(f, true)),
                    Err(e) if matches!(e.kind(), io::ErrorKind::AlreadyExists | io::ErrorKind::NotFound) => Ok(Opened::Retry),
                    Err(e) => Err(io_err(path, e)),
                }
            }
            Err(e) => Err(io_err(path, e)),
            Ok(m) if m.file_type().is_file() => match opts.open(path) {
                Ok(f) => Ok(Opened::File(f, false)),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Opened::Retry),
                Err(e) => Err(io_err(path, e)),
            },
            Ok(_) => Err(refuse(path, "not a regular file")),
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

    /// Gives up: closes the handle, removes what this acquisition made.
    fn abandon(file: File, created_file: bool, lock_path: &Path, created_cache: bool, cache: &Path) {
        drop(file);
        if created_file {
            let _ = fs::remove_file(lock_path);
        }
        if created_cache {
            let _ = fs::remove_dir(cache);
        }
    }

    pub(crate) fn acquire(ws: &Workspace, me: &Holder, wait: Duration) -> Result<WriteLock, LockError> {
        if ws.read_only().is_some() {
            return Err(LockError::ReadOnly);
        }
        let cache = ws.dir().join(".cache");
        let lock_path = ws.dir().join(LOCK_FILE);
        let record_path = ws.dir().join(LOCK_RECORD);
        let deadline = Instant::now().checked_add(wait.min(MAX_WAIT));
        let mut created_cache = false;
        let mut attempts = 0u32;
        let fail = |e: LockError, created_cache: bool| {
            if created_cache {
                let _ = fs::remove_dir(&cache);
            }
            e
        };
        let file = 'outer: loop {
            attempts += 1;
            if attempts > MAX_IDENTITY_RETRIES {
                return Err(fail(refuse(&lock_path, "the lock file keeps changing"), created_cache));
            }
            match ensure_cache(ws) {
                Ok(c) => created_cache |= c,
                Err(e) => return Err(fail(e, created_cache)),
            }
            let (file, created_file) = match open_lock_file(&lock_path) {
                Ok(Opened::File(f, c)) => (f, c),
                Ok(Opened::Retry) => continue,
                Err(e) => return Err(fail(e, created_cache)),
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
                            abandon(file, created_file, &lock_path, created_cache, &cache);
                            return Err(LockError::Held { holder: read_record(ws) });
                        }
                        std::thread::sleep(left.map_or(pause, |l| pause.min(l)));
                        pause = (pause * 2).min(Duration::from_millis(100));
                    }
                    Tried::Unsupported => {
                        abandon(file, created_file, &lock_path, created_cache, &cache);
                        return Err(LockError::Unsupported);
                    }
                    Tried::Failed(e) => {
                        abandon(file, created_file, &lock_path, created_cache, &cache);
                        return Err(io_err(&lock_path, e));
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
        Ok(WriteLock { file: Some(file), lock_path, record_path, cache_path: cache, wrote_record, created_cache, stale })
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
