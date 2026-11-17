//! Shape session identity: the values the launcher's `detail.init(...)`
//! carries, extracted once per site/kernel build and cached with the layout.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};

/// Session facts the reference engine validates: token, base64 alphabet for
/// the custom payload encoding, and the 8 cipher key words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Transaction token from the page (`f` header channel / checksum salt).
    pub transaction_token: String,

    /// 64/65-char custom base64 alphabet. First 64 chars are data symbols.
    pub alphabet: String,

    /// Optional shuffle key applied to payload section order.
    #[serde(default)]
    pub shuffle_key: Vec<u32>,

    /// Eight u32 cipher key words seeded from the launcher.
    #[serde(default = "default_keys")]
    pub keys: [u32; 8],
}

fn default_keys() -> [u32; 8] {
    [0; 8]
}

impl Session {
    /// Validate the invariants the wire codecs rely on before use.
    pub fn validate(&self) -> Result<()> {
        if self.transaction_token.is_empty() {
            return Err(Error::Session("empty transaction token".into()));
        }
        let unique = self.alphabet.chars().count();
        if !(64..=65).contains(&unique) {
            return Err(Error::Session(format!(
                "alphabet must hold 64 data symbols (+1 pad), got {unique}"
            )));
        }
        let mut seen = [false; 256];
        for ch in self.alphabet.chars().take(64) {
            let b = u8::try_from(ch as u32)
                .map_err(|_| Error::Session("alphabet chars must be ASCII".into()))?;
            if seen[b as usize] {
                return Err(Error::Session(format!("duplicate alphabet symbol {ch}")));
            }
            seen[b as usize] = true;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALPHA: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    fn base() -> Session {
        Session {
            transaction_token: "ab".into(),
            alphabet: ALPHA.into(),
            shuffle_key: vec![],
            keys: [0; 8],
        }
    }

    #[test]
    fn accepts_reference_session() {
        base().validate().unwrap();
    }

    #[test]
    fn rejects_short_alphabet_and_dupes() {
        let mut s = base();
        s.alphabet = "ABC".into();
        assert!(s.validate().is_err());
        let mut s = base();
        s.alphabet = "AABCDEF".repeat(10);
        assert!(s.validate().is_err());
        let mut s = base();
        s.transaction_token = String::new();
        assert!(s.validate().is_err());
    }
}
