//! Signal catalog: the semantic registry of every signal a Shape build can
//! collect, its wire kind and the synthesizer hook that produces values.
//!
//! Names mirror the reference `src/signals/catalog/*` modules (the de-facto
//! schema vocabulary), but the registry, defaults and synthesis live here.

use serde::{Deserialize, Serialize};

/// Wire representation of one signal's payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WireKind {
    /// Packed boolean bits, MSB-first in one byte.
    Flags,
    /// sign-magnitude-5-7 integer.
    Integer,
    /// IEEE binary64 via float path or integer arm.
    Number,
    /// modified-utf8-intern-129 string.
    String,
    /// Raw byte blob (hashes, canvas samples).
    Bytes,
}

/// Emission routing inside the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Routing {
    /// Fixed positional slot assigned at pin time.
    Positional,
    /// u32 key in the keyed channel.
    Keyed(u32),
}

/// One collectable signal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalDef {
    /// Canonical snake_case name (schema key in camelCase on the wire).
    pub name: &'static str,

    /// What the sensor observes, one line.
    pub description: &'static str,

    pub kind: WireKind,

    /// true when the value is stable per device (cacheable in the profile);
    /// false when it is session-flavoured (timings, counters) and the
    /// synthesizer jitters it per request.
    pub device_static: bool,

    pub routing: Routing,
}

impl SignalDef {
    pub const fn keyed(name: &'static str, key: u32) -> Self {
        // Placeholder for const-friendly definitions; full defs live in decl!.
        Self {
            name,
            description: "",
            kind: WireKind::Integer,
            device_static: true,
            routing: Routing::Keyed(key),
        }
    }
}

/// The bootstrap subset: the signals the reference smoke fixtures exercise.
/// The full 90-signal table lands with the bandai pin (schema-driven, not
/// hand-transcribed — the pinned artifact names exactly what that build sends).
pub const BOOTSTRAP: &[SignalDef] = &[
    SignalDef {
        name: "device_memory",
        description: "navigator.deviceMemory GiB",
        kind: WireKind::Number,
        device_static: true,
        routing: Routing::Positional,
    },
    SignalDef {
        name: "storage_access_api_presence",
        description: "hasStorageAccess/requestStorageAccess in document",
        kind: WireKind::Flags,
        device_static: false,
        routing: Routing::Positional,
    },
    SignalDef {
        name: "constant_empty_string",
        description: "liveness canary, always empty string",
        kind: WireKind::String,
        device_static: true,
        routing: Routing::Positional,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_names_are_unique() {
        let mut names: Vec<_> = BOOTSTRAP.iter().map(|s| s.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), BOOTSTRAP.len());
    }
}
