//! Content hashes: BLAKE3-256, written as 64 lowercase hexadecimal characters.

use std::fmt;

/// The BLAKE3 hash of a piece of content in its canonical encoding.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Hashes the concatenation of `parts`.
    pub fn of(parts: &[&[u8]]) -> Self {
        let mut hasher = blake3::Hasher::new();
        for part in parts {
            hasher.update(part);
        }
        Self(*hasher.finalize().as_bytes())
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(64);
        for byte in self.0 {
            out.push(char::from_digit((byte >> 4) as u32, 16).expect("nibble"));
            out.push(char::from_digit((byte & 15) as u32, 16).expect("nibble"));
        }
        out
    }

    /// Parses exactly 64 lowercase hexadecimal characters.
    pub fn from_hex(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if bytes.len() != 64 {
            return None;
        }
        let nibble = |c: u8| match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            _ => None,
        };
        let mut out = [0u8; 32];
        for (i, pair) in bytes.as_chunks::<2>().0.iter().enumerate() {
            out[i] = (nibble(pair[0])? << 4) | nibble(pair[1])?;
        }
        Some(Self(out))
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContentHash({})", self.to_hex())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips_and_rejects_anything_else() {
        let hash = ContentHash::of(&[b"abc", b"def"]);
        assert_eq!(hash, ContentHash::of(&[b"abcdef"]));
        let hex = hash.to_hex();
        assert_eq!(hex.len(), 64);
        assert_eq!(ContentHash::from_hex(&hex), Some(hash));
        assert_eq!(ContentHash::from_hex(&hex.to_uppercase()), None);
        assert_eq!(ContentHash::from_hex(&hex[..62]), None);
        assert_eq!(ContentHash::from_hex(&format!("{}zz", &hex[..62])), None);
    }

    #[test]
    fn matches_the_published_blake3_test_vector() {
        // BLAKE3 of the empty input, from the reference test vectors.
        assert_eq!(
            ContentHash::of(&[]).to_hex(),
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );
    }
}
