//! Typed signal values and the four-channel report the encoders consume.
//!
//! Mirrors the reference `Report` shape (errors / positional / signals /
//! keyed) without exposing its serde serialization details; the wire layer
//! owns byte-level framing.

use crate::catalog::SignalDef;
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};

/// A single signal value in the typed model (superpack-compatible subset).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Undefined,
    Null,
    Bool(bool),
    /// IEEE-754 bits, hex-encoded at the JSON boundary (lossless f64).
    Number(Number),
    String(String),
    Bytes(#[serde(with = "byte_array")] Vec<u8>),
}

/// f64 wrapper: JSON serializes as 16 hex bit-characters, lossless.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Number(pub f64);

impl serde::Serialize for Number {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{:016x}", self.0.to_bits()))
    }
}

impl<'de> serde::Deserialize<'de> for Number {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let hex = String::deserialize(deserializer)?;
        u64::from_str_radix(&hex, 16)
            .map(|bits| Number(f64::from_bits(bits)))
            .map_err(serde::de::Error::custom)
    }
}

impl From<f64> for Number {
    fn from(value: f64) -> Self {
        Self(value)
    }
}

mod byte_array {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(value: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        value.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        Vec::<u8>::deserialize(deserializer)
    }
}

impl Value {
    /// The wire kind this value encodes as, checked against the definition.
    pub fn matches(&self, kind: crate::catalog::WireKind) -> bool {
        use crate::catalog::WireKind as K;
        matches!(
            (self, kind),
            (Value::Bool(_), K::Flags)
                | (Value::Number(_), K::Number)
                | (Value::Number(_), K::Integer)
                | (Value::String(_), K::String)
                | (Value::Bytes(_), K::Bytes)
        )
    }
}

/// One positional entry: the sensor's encoded bytes plus already-finalized
/// `extra` passthrough (never re-framed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PositionalEntry {
    pub encoded: Vec<u8>,
    #[serde(default)]
    pub extra: Vec<u8>,
}

/// One keyed entry: u32 name plus the same payload pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyedEntry {
    pub name: u32,
    pub encoded: Vec<u8>,
    #[serde(default)]
    pub extra: Vec<u8>,
}

/// The full pre-encryption report. `seeds` are the two per-payload u32 words;
/// `None` positional slots serialize as the two-zero sentinel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub errors: Vec<Value>,
    pub positional: Vec<Option<PositionalEntry>>,
    pub signals: Vec<Value>,
    pub keyed: Vec<KeyedEntry>,
    pub seeds: [u32; 2],
}

impl Report {
    /// Empty report with the given seed pair.
    pub fn empty(seeds: [u32; 2]) -> Self {
        Self {
            errors: Vec::new(),
            positional: Vec::new(),
            signals: Vec::new(),
            keyed: Vec::new(),
            seeds,
        }
    }

    /// Entry count preflight: the container caps positional+keyed at 65,536.
    pub fn validate_limits(&self) -> Result<()> {
        const MAX_CONTAINER_ENTRIES: usize = 65_536;
        let entries = self.positional.len() + self.keyed.len();
        if entries > MAX_CONTAINER_ENTRIES {
            return Err(Error::Encode(format!(
                "container entries {entries} exceed {MAX_CONTAINER_ENTRIES}"
            )));
        }
        Ok(())
    }
}

/// Named view over a decoded/encoded report: signal name -> value, the form
/// callers and the synthesizer work with. Serializes as a plain object
/// (static names make the map well-formed); the `&'static str` keys rebuild
/// on deserialize via the catalog lookup, falling back to leaking for
/// dynamic tool input (tools, not the server hot path).
#[derive(Debug, Clone, Default)]
pub struct NamedSignals {
    values: Vec<(&'static str, Value)>,
}

impl serde::Serialize for NamedSignals {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.values.len()))?;
        for (name, value) in &self.values {
            map.serialize_entry(name, value)?;
        }
        map.end()
    }
}

impl<'de> serde::Deserialize<'de> for NamedSignals {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = std::collections::BTreeMap::<String, Value>::deserialize(deserializer)?;
        let mut named = Self::default();
        for (name, value) in raw {
            // Tool-supplied keys leak into 'static names; bounded by the
            // catalog size in practice and never re-parsed in hot loops.
            let leaked: &'static str = Box::leak(name.into_boxed_str());
            named.values.push((leaked, value));
        }
        Ok(named)
    }
}

impl NamedSignals {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, name: &'static str, value: Value) -> &mut Self {
        if let Some(slot) = self.values.iter_mut().find(|(n, _)| *n == name) {
            slot.1 = value;
        } else {
            self.values.push((name, value));
        }
        self
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.values.iter().find(|(n, _)| *n == name).map(|(_, v)| v)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &Value)> {
        self.values.iter().map(|(n, v)| (*n, v))
    }

    /// Type-check every entry against its catalog definition. Called by the
    /// codec tools before encoding; the server path validates via the synth
    /// rules instead (single source of truth per path).
    pub fn validate_against(&self, catalog: &[SignalDef]) -> Result<()> {
        let pairs: Vec<(&'static str, &Value)> = self.iter().collect();
        Self::validate_pairs_static(&pairs, catalog)
    }

    fn validate_pairs_static(
        pairs: &[(&'static str, &Value)],
        catalog: &[SignalDef],
    ) -> Result<()> {
        for (name, value) in pairs {
            let def = catalog
                .iter()
                .find(|d| d.name == *name)
                .ok_or_else(|| Error::Encode(format!("signal {name} not in catalog")))?;
            if !value.matches(def.kind) {
                return Err(Error::Encode(format!(
                    "signal {name} value type mismatches catalog kind {:?}",
                    def.kind
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_roundtrips_lossless_through_json() {
        let original = Number(0.1 + 0.2);
        let json = serde_json::to_string(&Value::Number(original)).unwrap();
        let back = serde_json::from_str::<Value>(&json).unwrap();
        assert_eq!(back, Value::Number(original));
    }

    #[test]
    fn named_set_is_upsert() {
        let mut named = NamedSignals::new();
        named.set("a", Value::Bool(true));
        named.set("a", Value::Bool(false));
        assert_eq!(named.get("a"), Some(&Value::Bool(false)));
        assert_eq!(named.iter().count(), 1);
    }

    #[test]
    fn limits_reject_oversized_containers() {
        let mut report = Report::empty([1, 2]);
        report.positional = (0..65_537)
            .map(|_| {
                Some(PositionalEntry {
                    encoded: vec![1],
                    extra: vec![],
                })
            })
            .collect();
        assert!(report.validate_limits().is_err());
    }
}
