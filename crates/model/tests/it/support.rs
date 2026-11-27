//! Test sources and helpers for the id tests.

use notecraft_model::{Clock, Entropy, EntropyError, Id};

/// A fixed millisecond in 2026. Tests compare against it, never against a literal.
pub const T0: u64 = 1_785_000_000_000;

/// Always returns the same time.
pub struct FixedClock(pub u64);

impl Clock for FixedClock {
    fn now_ms(&mut self) -> Option<u64> {
        Some(self.0)
    }
}

/// Returns each time in turn, then repeats the last one.
pub struct ScriptedClock {
    pub times: Vec<u64>,
    pub at: usize,
}

impl ScriptedClock {
    pub fn new(times: Vec<u64>) -> Self {
        Self { times, at: 0 }
    }
}

impl Clock for ScriptedClock {
    fn now_ms(&mut self) -> Option<u64> {
        let last = self.times.len().checked_sub(1)?;
        let time = self.times.get(self.at.min(last)).copied();
        self.at = self.at.saturating_add(1);
        time
    }
}

/// A clock that has no usable time.
pub struct NoClock;

impl Clock for NoClock {
    fn now_ms(&mut self) -> Option<u64> {
        None
    }
}

/// Fills every buffer with one byte value.
pub struct ConstEntropy(pub u8);

impl Entropy for ConstEntropy {
    fn fill(&mut self, buf: &mut [u8]) -> Result<(), EntropyError> {
        buf.fill(self.0);
        Ok(())
    }
}

/// An entropy source that always fails.
pub struct NoEntropy;

impl Entropy for NoEntropy {
    fn fill(&mut self, _buf: &mut [u8]) -> Result<(), EntropyError> {
        Err(EntropyError)
    }
}

/// Fails the first call, then delegates.
pub struct FailOnce<E> {
    failed: bool,
    inner: E,
}

impl<E> FailOnce<E> {
    pub fn new(inner: E) -> Self {
        Self { failed: false, inner }
    }
}

impl<E: Entropy> Entropy for FailOnce<E> {
    fn fill(&mut self, buf: &mut [u8]) -> Result<(), EntropyError> {
        if self.failed {
            self.inner.fill(buf)
        } else {
            self.failed = true;
            Err(EntropyError)
        }
    }
}

/// Fails the call with index `n` (0-based, counting every call), and delegates otherwise.
pub struct FailOnCall<E> {
    calls: usize,
    n: usize,
    inner: E,
}

impl<E> FailOnCall<E> {
    pub fn new(n: usize, inner: E) -> Self {
        Self { calls: 0, n, inner }
    }
}

impl<E: Entropy> Entropy for FailOnCall<E> {
    fn fill(&mut self, buf: &mut [u8]) -> Result<(), EntropyError> {
        let call = self.calls;
        self.calls = self.calls.saturating_add(1);
        if call == self.n { Err(EntropyError) } else { self.inner.fill(buf) }
    }
}

/// 26 characters, all from the upper-case Crockford alphabet, first one `0`-`7`.
pub fn is_canonical(text: &str) -> bool {
    const ALPHABET: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    text.len() == Id::LEN && text.chars().all(|c| ALPHABET.contains(c)) && text.chars().next().is_some_and(|c| ('0'..='7').contains(&c))
}

/// The 48-bit millisecond timestamp, read from the big-endian bytes.
pub fn timestamp_ms(id: Id) -> u64 {
    id.to_bytes().iter().take(6).fold(0u64, |acc, byte| (acc << 8) | u64::from(*byte))
}

/// `(version, variant)`: the high nibble of byte 6 and the top two bits of byte 8.
pub fn version_variant(id: Id) -> (u8, u8) {
    let bytes = id.to_bytes();
    let version = bytes.get(6).copied().unwrap_or(0) >> 4;
    let variant = bytes.get(8).copied().unwrap_or(0) >> 6;
    (version, variant)
}
