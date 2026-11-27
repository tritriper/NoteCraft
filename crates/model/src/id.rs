//! The `Id` value type: 128 bits, written as 26 Crockford base32 characters (ADR 0004).

use std::fmt;
use std::str::FromStr;

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// A stable item id: a 128-bit value, written as 26 Crockford base32 characters.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Id(u128);

/// Why a string is not an id. It never holds the input, so messages stay short.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IdError {
    /// The byte length is not 26.
    Length { found: usize },
    /// The byte at `index` is outside the alphabet.
    Char { index: usize },
    /// The first character is above `7`: the value needs more than 128 bits.
    Overflow,
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdError::Length { found } => write!(f, "an id has {} bytes, found {found}", Id::LEN),
            IdError::Char { index } => write!(f, "invalid character in id at byte {index}"),
            IdError::Overflow => write!(f, "id is too large: the first character must be 0 to 7"),
        }
    }
}

impl std::error::Error for IdError {}

/// Value of one Crockford digit, any case; `None` outside the alphabet (aliases included).
fn decode(byte: u8) -> Option<u8> {
    let upper = byte.to_ascii_uppercase();
    ALPHABET.iter().position(|&c| c == upper).and_then(|p| u8::try_from(p).ok())
}

impl Id {
    /// Characters in the text form.
    pub const LEN: usize = 26;

    /// Reads an id in any case; strict otherwise (ADR 0004 decision 3).
    pub fn parse(text: &str) -> Result<Id, IdError> {
        let bytes = text.as_bytes();
        if bytes.len() != Self::LEN {
            return Err(IdError::Length { found: bytes.len() });
        }
        let mut value: u128 = 0;
        let mut first = 0u8;
        for (index, &byte) in bytes.iter().enumerate() {
            let digit = decode(byte).ok_or(IdError::Char { index })?;
            if index == 0 {
                first = digit;
            }
            value = (value << 5) | u128::from(digit);
        }
        if first > 7 {
            return Err(IdError::Overflow);
        }
        Ok(Id(value))
    }

    /// An id from its big-endian (UUID order) bytes.
    pub const fn from_bytes(bytes: [u8; 16]) -> Id {
        Id(u128::from_be_bytes(bytes))
    }

    /// The big-endian (UUID order) bytes.
    pub const fn to_bytes(self) -> [u8; 16] {
        self.0.to_be_bytes()
    }

    /// The 48-bit millisecond timestamp of a UUIDv7 id (its top 48 bits).
    pub const fn timestamp_ms(self) -> u64 {
        (self.0 >> 80) as u64
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for i in 0..Id::LEN {
            let shift = 5 * (Id::LEN - 1 - i);
            let digit = usize::try_from((self.0 >> shift) & 31).unwrap_or(0);
            let byte = ALPHABET.get(digit).copied().unwrap_or(b'0');
            fmt::Write::write_char(f, char::from(byte))?;
        }
        Ok(())
    }
}

impl FromStr for Id {
    type Err = IdError;
    fn from_str(text: &str) -> Result<Id, IdError> {
        Id::parse(text)
    }
}
