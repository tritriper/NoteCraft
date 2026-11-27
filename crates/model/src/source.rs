//! Time and randomness ports, and their system implementations that never panic (ADR 0004 #7).

use std::fmt;

/// A source of wall-clock time.
pub trait Clock {
    /// Milliseconds since the Unix epoch, or `None` when there is no usable time.
    fn now_ms(&mut self) -> Option<u64>;
}

/// A source of random bytes.
pub trait Entropy {
    /// Fills `buf` with random bytes, or fails.
    fn fill(&mut self, buf: &mut [u8]) -> Result<(), EntropyError>;
}

/// The entropy source failed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EntropyError;

impl fmt::Display for EntropyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("no randomness available")
    }
}

impl std::error::Error for EntropyError {}

/// The system clock: `None` before 1970 or on a bad value, never a panic.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
impl Clock for SystemClock {
    fn now_ms(&mut self) -> Option<u64> {
        let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?;
        u64::try_from(since.as_millis()).ok()
    }
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
impl Clock for SystemClock {
    fn now_ms(&mut self) -> Option<u64> {
        let now = js_sys::Date::now();
        // Finite, non-negative and below 2^53, so the conversion is exact.
        if now.is_finite() && (0.0..9.0e15).contains(&now) { Some(now as u64) } else { None }
    }
}

/// Operating-system randomness (`getrandom`).
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemEntropy;

impl Entropy for SystemEntropy {
    fn fill(&mut self, buf: &mut [u8]) -> Result<(), EntropyError> {
        getrandom::fill(buf).map_err(|_| EntropyError)
    }
}
