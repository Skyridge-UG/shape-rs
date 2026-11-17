//! Seed layout: the loaded pinned artifact + derived encoders.
//!
//! A layout is the pinned product of one (kernel source, session) pair: which
//! signals the build emits, in which wire grammar, with which cipher geometry.
//! Loading validates the format tag and the internal consistency the encoders
//! rely on; byte-level encode/decode delegates to the vendored engine behind
//! the `engine` feature (see `pin.rs`).

use crate::error::{Error, Result};
use crate::session::Session;
use serde::{Deserialize, Serialize};

/// Format tag written by the pin stage.
pub const SEED_LAYOUT_FORMAT: &str = "shape-seed-layout/1";

/// Session section as stored inside a layout artifact (validated subset).
/// Serde names match the reference artifact's camelCase exactly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutSession {
    pub transaction_token: String,
    pub alphabet: String,
    #[serde(default)]
    pub shuffle_key: Vec<u32>,
    #[serde(default)]
    pub keys: [u32; 8],
}

impl TryFrom<LayoutSession> for Session {
    type Error = Error;

    fn try_from(value: LayoutSession) -> Result<Self> {
        let session = Self {
            transaction_token: value.transaction_token,
            alphabet: value.alphabet,
            shuffle_key: value.shuffle_key,
            keys: value.keys,
        };
        session.validate()?;
        Ok(session)
    }
}

/// Cipher geometry carried from the pinned build.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CipherLayout {
    pub ivs: [u32; 4],
    #[serde(default)]
    pub counter: u64,
    #[serde(default = "default_double_rounds")]
    pub double_rounds: u32,
}

fn default_double_rounds() -> u32 {
    10
}

impl CipherLayout {
    /// The reference profile bounds: counter < 2^53, 1..=20 double rounds.
    pub fn validate(&self) -> Result<()> {
        if self.counter >= (1u64 << 53) {
            return Err(Error::Layout(format!(
                "counter {} exceeds 2^53",
                self.counter
            )));
        }
        if !(1..=20).contains(&self.double_rounds) {
            return Err(Error::Layout(format!(
                "double_rounds {} outside 1..=20",
                self.double_rounds
            )));
        }
        Ok(())
    }
}

/// Header naming policy pinned from the build (`x-<site>-<n>` chunks).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderLayout {
    /// Full `a`-channel header name, e.g. `x-example-a`.
    pub name: String,

    /// Prefix shared by every chunk (`x-example-`).
    pub prefix: String,

    /// Maximum chars per `a` chunk.
    #[serde(default = "default_chunk")]
    pub chunk_size: usize,

    /// Sibling channels: `b`/`f` are presence flags; `c`/`d`/`z` are the
    /// pinned default strings the engine's HeaderLayout carries (empty
    /// string = present-with-empty-default, matching the pinned artifacts).
    #[serde(default)]
    pub b: bool,
    #[serde(default)]
    pub c: String,
    #[serde(default)]
    pub d: String,
    #[serde(default)]
    pub f: bool,
    #[serde(default)]
    pub z: String,
}

fn default_chunk() -> usize {
    1000
}

/// The loaded layout (engine-artifact compatible: nested `source` object,
/// `keysets`/`report` accepted-and-ignored for forward compatibility).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedLayout {
    pub format: String,
    #[serde(default)]
    pub source: Option<SourceRef>,
    pub session: LayoutSession,
    pub cipher: CipherLayout,
    pub header: HeaderLayout,
    #[serde(default)]
    pub keysets: Vec<Vec<String>>,
    #[serde(default)]
    pub report: serde_json::Value,
}

/// Source digest reference inside an engine artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub bytes: u64,
}

impl SeedLayout {
    /// sha256 of the pinned source, from either artifact shape.
    pub fn source_sha256(&self) -> &str {
        match &self.source {
            Some(s) => &s.sha256,
            None => "",
        }
    }

    /// Pinned source byte length, from either artifact shape.
    pub fn source_bytes(&self) -> u64 {
        match &self.source {
            Some(s) => s.bytes,
            None => 0,
        }
    }
    /// The signal-slot surface a kernel change alters: our crate defines the
    /// catalog slots implicitly by synthesis order, so the map is the catalog
    /// bootstrap indexed by routing (the positional contract). Tools print
    /// this to diff kernel generations.
    pub fn signal_slots(&self) -> Vec<serde_json::Value> {
        crate::catalog::BOOTSTRAP
            .iter()
            .enumerate()
            .map(|(index, def)| {
                serde_json::json!({
                    "slot": index,
                    "name": def.name,
                    "kind": def.kind,
                    "device_static": def.device_static,
                })
            })
            .collect()
    }

    /// Parse + validate a layout artifact (JSON bytes as written by `pin`).
    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        let layout: Self = serde_json::from_slice(bytes)
            .map_err(|e| Error::Layout(format!("artifact JSON: {e}")))?;
        layout.validate()?;
        Ok(layout)
    }

    pub fn validate(&self) -> Result<()> {
        if self.format != SEED_LAYOUT_FORMAT {
            return Err(Error::Layout(format!(
                "format {} != {SEED_LAYOUT_FORMAT}",
                self.format
            )));
        }
        let session = Session::try_from(self.session.clone())?;
        session.validate()?;
        self.cipher.validate()?;
        if self.header.name.is_empty() || !self.header.name.starts_with(&self.header.prefix) {
            return Err(Error::Layout(
                "header name must extend the pinned prefix".into(),
            ));
        }
        if self.header.chunk_size == 0 {
            return Err(Error::Layout("chunk_size must be positive".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> serde_json::Value {
        serde_json::json!({
            "format": SEED_LAYOUT_FORMAT,
            "session": {
                "transactionToken": "ab",
                "alphabet": "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
                "shuffleKey": [],
                "keys": [0, 1, 2, 3, 4, 5, 6, 7],
            },
            "cipher": { "ivs": [1, 2, 3, 4], "counter": 5, "doubleRounds": 1 },
            "header": { "name": "x-example-a", "prefix": "x-example-", "chunkSize": 1000 },
            "sourceSha256": "aa".repeat(32),
            "sourceBytes": 8111,
        })
    }

    #[test]
    fn accepts_wellformed_layout() {
        let layout: SeedLayout =
            serde_json::from_value(base()).expect("well-formed fixture");
        layout.validate().unwrap();
        let round = SeedLayout::from_json(&serde_json::to_vec(&layout).unwrap()).unwrap();
        assert_eq!(round, layout);
    }

    #[test]
    fn rejects_bad_format_and_bad_counter() {
        let mut v = base();
        v["format"] = serde_json::json!("nope/9");
        let layout: SeedLayout = serde_json::from_value(v).unwrap();
        assert!(layout.validate().is_err());

        let mut v = base();
        v["cipher"]["counter"] = serde_json::json!(1u64 << 53);
        let layout: SeedLayout = serde_json::from_value(v).unwrap();
        assert!(layout.validate().is_err());
    }

    #[test]
    fn rejects_header_prefix_mismatch() {
        let mut v = base();
        v["header"]["name"] = serde_json::json!("x-other-a");
        let layout: SeedLayout = serde_json::from_value(v).unwrap();
        assert!(layout.validate().is_err());
    }
}
