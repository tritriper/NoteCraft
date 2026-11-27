//! Monotonic UUIDv7 generator (ADR 0004 decision 5).

use std::fmt;

use crate::id::Id;
use crate::source::{Clock, Entropy, SystemClock, SystemEntropy};

/// Why an id could not be generated.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GenError {
    /// No usable time.
    Clock,
    /// No randomness.
    Entropy,
}

impl fmt::Display for GenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GenError::Clock => f.write_str("cannot generate an id: no usable clock"),
            GenError::Entropy => f.write_str("cannot generate an id: no randomness available"),
        }
    }
}

impl std::error::Error for GenError {}

const TS_MAX: u64 = (1 << 48) - 1;
const COUNTER_MASK: u128 = (1u128 << 74) - 1;
const LOW_BITS: u32 = 62;

/// Generates ids that increase strictly within one generator.
pub struct IdGen<C: Clock = SystemClock, E: Entropy = SystemEntropy> {
    clock: C,
    entropy: E,
    /// Last timestamp and 74-bit counter used.
    last: Option<(u64, u128)>,
}

impl IdGen {
    /// A generator over the system clock and system randomness.
    pub fn new() -> Self {
        Self::with_sources(SystemClock, SystemEntropy)
    }
}

impl Default for IdGen {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Clock, E: Entropy> IdGen<C, E> {
    /// A generator over the given sources.
    pub fn with_sources(clock: C, entropy: E) -> Self {
        Self { clock, entropy, last: None }
    }

    fn fresh_counter(&mut self) -> Result<u128, GenError> {
        let mut buf = [0u8; 16];
        let head = buf.get_mut(6..).ok_or(GenError::Entropy)?;
        self.entropy.fill(head).map_err(|_| GenError::Entropy)?;
        Ok(u128::from_be_bytes(buf) & COUNTER_MASK)
    }

    /// The next id. On error the generator is unchanged.
    pub fn generate(&mut self) -> Result<Id, GenError> {
        let now = self.clock.now_ms().ok_or(GenError::Clock)?;
        let (ts, counter) = match self.last {
            Some((ts, counter)) if now <= ts => match counter.checked_add(1) {
                Some(next) if next <= COUNTER_MASK => (ts, next),
                _ => (ts.checked_add(1).ok_or(GenError::Clock)?, self.fresh_counter()?),
            },
            _ => (now, self.fresh_counter()?),
        };
        if ts > TS_MAX {
            return Err(GenError::Clock);
        }
        self.last = Some((ts, counter));
        let rand_a = counter >> LOW_BITS;
        let rand_b = counter & ((1u128 << LOW_BITS) - 1);
        let value = (u128::from(ts) << 80) | (0x7u128 << 76) | (rand_a << 64) | (0b10u128 << LOW_BITS) | rand_b;
        Ok(Id::from_bytes(value.to_be_bytes()))
    }
}
