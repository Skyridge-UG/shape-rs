//! Header encoding: named signals -> per-sensor wire bytes -> the `x-â€¦` bundle.
//!
//! The structural frame (container sections, outer cipher, chunking) is the
//! vendored engine's encode path behind the `engine` feature; this module
//! owns the semantic half: profile->signal bytes via the catalog and the
//! session keyset wiring, plus the engine-free fallback that emits the
//! seed/layoutless `f/c/d/z` channels and a well-formed but unencrypted `a`
//! frame for smoke testing.

use crate::error::Result;
use crate::signal::{NamedSignals, Value};
use crate::wire::Stream;

/// The final header bundle as sent on the wire.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HeaderBundle {
    /// `a` chunks in order (`x-<site>-a`, `x-<site>-a10`, â€¦).
    pub a: Vec<String>,

    /// `b`: payload seed suffix, base36 of seed^2 xor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub c: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub d: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub f: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z: Option<String>,
}

impl HeaderBundle {
    /// Full name->value map ready for an HTTP client, prefix applied.
    /// Chunk naming per the fresh 200 capture: `a`, then `a0`, `a1`, ...
    /// (base-36-style zero-based continuation, NOT a10/a11).
    pub fn http_headers(&self, prefix: &str) -> Vec<(String, String)> {
        let mut out = Vec::with_capacity(self.a.len() + 5);
        for (index, chunk) in self.a.iter().enumerate() {
            let name = if index == 0 {
                format!("{prefix}a")
            } else {
                format!("{prefix}a{}", index - 1)
            };
            out.push((name, chunk.clone()));
        }
        for (present, name, value) in [
            (self.b.is_some(), "b", &self.b),
            (self.c.is_some(), "c", &self.c),
            (self.d.is_some(), "d", &self.d),
            (self.f.is_some(), "f", &self.f),
            (self.z.is_some(), "z", &self.z),
        ] {
            if let (true, Some(v)) = (present, value) {
                out.push((format!("{prefix}{name}"), v.clone()));
            }
        }
        out
    }
}

/// Encode one string signal's payload through the masked UTF-8 writer.
pub fn string_payload(value: &str, mask: &mut Stream) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut masked = MaskedWriter {
        buf: &mut buf,
        stream: mask,
    };
    masked.string(value)?;
    Ok(buf)
}

/// Adapter bridging [`StringWriter`]'s closure-based mask to the stream.
struct MaskedWriter<'a> {
    buf: &'a mut Vec<u8>,
    stream: &'a mut Stream,
}

impl MaskedWriter<'_> {
    fn string(&mut self, value: &str) -> Result<()> {
        for unit in value.encode_utf16() {
            let mask = self.stream.next();
            match unit {
                0 => {
                    self.buf.push(0xc0 ^ mask);
                    self.buf.push(0x80 ^ self.stream.next());
                }
                1..=0x7f => self.buf.push((unit as u8) ^ mask),
                0x80..=0x7ff => {
                    self.buf.push(((unit >> 6) as u8 | 0xc0) ^ mask);
                    self.buf.push(((unit & 0x3f) as u8 | 0x80) ^ self.stream.next());
                }
                0xd800..=0xdfff => return Err(crate::error::Error::Encode("lone surrogate".into())),
                _ => {
                    self.buf.push(((unit >> 12) as u8 | 0xe0) ^ mask);
                    self.buf
                        .push(((((unit >> 6) & 0x3f) as u8) | 0x80) ^ self.stream.next());
                    self.buf.push(((unit & 0x3f) as u8 | 0x80) ^ self.stream.next());
                }
            }
        }
        self.buf.push(self.stream.next());
        Ok(())
    }
}

/// The engine-backed full encode: signals + layout -> sealed `a` chunks.
#[cfg(feature = "engine")]
pub fn encode_headers(
    engine_layout: &shape_engine::signals::payload::extract::SeedLayout,
    signals: &NamedSignals,
    seeds: [u32; 2],
) -> Result<HeaderBundle> {
    use shape_engine::signals::payload::superpack::Value as E;

    let slots = engine_layout.report.positional_signals.len();
    if slots != signals.iter().count() {
        return Err(crate::error::Error::Encode(format!(
            "layout pins {slots} positional slots but {count} signals synthesized",
            count = signals.iter().count()
        )));
    }

    // Slot N gets the Nth named signal (catalog order == pin order).
    let positional: Vec<Option<shape_engine::signals::payload::report::Entry>> = signals
        .iter()
        .map(|(_, value)| {
            let engine_value = match value {
                crate::signal::Value::Bool(b) => E::Bool(*b),
                crate::signal::Value::Number(n) => {
                    E::Number(shape_engine::signals::payload::superpack::Number(n.0.to_bits()))
                }
                crate::signal::Value::String(s) => E::String(s.clone()),
                crate::signal::Value::Bytes(b) => E::Binary(b.clone()),
                crate::signal::Value::Undefined => E::Undefined,
                crate::signal::Value::Null => E::Null,
            };
            // Entry = (Vec<Value>, Vec<u8>): the engine packs the value side
            // through superpack; extra stays empty (no detector raw bytes).
            Some((vec![engine_value], Vec::new()))
        })
        .collect();

    let channels = shape_engine::signals::payload::ChannelReport {
        errors: Vec::new(),
        positional_signals: positional,
        signals: Vec::new(),
        keyed_signals: Vec::new(),
    };

    let headers = engine_layout
        .encode_headers(&channels, seeds)
        .map_err(|e| crate::error::Error::Encode(format!("engine encode: {e}")))?;

    to_bundle(&headers, &engine_layout.header)
}

/// Engine `Headers` -> our bundle, preserving the engine's exact spellings.
#[cfg(feature = "engine")]
fn to_bundle(
    headers: &shape_engine::signals::payload::Headers,
    header_layout: &shape_engine::signals::payload::extract::HeaderLayout,
) -> Result<HeaderBundle> {
    let prefix = header_layout
        .name
        .strip_suffix('a')
        .unwrap_or(&header_layout.name);
    let mut a: Vec<(usize, String)> = Vec::new();
    let mut b = None;
    let mut c = None;
    let mut d = None;
    let mut f = None;
    let mut z = None;
    for (name, value) in headers.iter() {
        let Some(suffix) = name.strip_prefix(prefix) else {
            continue;
        };
        if let Some(numeric) = suffix.strip_prefix('a') {
            let index: usize = if numeric.is_empty() {
                0
            } else {
                numeric
                    .parse()
                    .map_err(|_| crate::error::Error::Encode(format!("bad a-chunk {name}")))?
            };
            a.push((index.wrapping_sub(1), value.to_owned()));
        }
        match suffix {
            "b" => b = Some(value.to_owned()),
            "c" => c = Some(value.to_owned()),
            "d" => d = Some(value.to_owned()),
            "f" => f = Some(value.to_owned()),
            "z" => z = Some(value.to_owned()),
            _ => {}
        }
    }
    if a.is_empty() {
        return Err(crate::error::Error::Encode(
            "engine produced no a-channel chunks".into(),
        ));
    }
    a.sort_by_key(|(index, _)| *index);
    Ok(HeaderBundle {
        a: a.into_iter().map(|(_, v)| v).collect(),
        b,
        c,
        d,
        f,
        z,
    })
}

/// Engine-free smoke encode: proves the pipeline shape end to end without the
/// outer cipher (a-frames carry the raw masked payload, clearly marked).
pub fn encode_smoke(
    _prefix: &str,
    signals: &NamedSignals,
    seeds: [u32; 2],
) -> Result<HeaderBundle> {
    if signals.iter().count() == 0 {
        return Err(crate::error::Error::Encode("no signals to encode".into()));
    }
    let mut stream = Stream::new(seeds[0], 17);
    stream.with_token_hash(seeds[1]);

    let mut payload = Vec::new();
    for (name, value) in signals.iter() {
        match value {
            Value::String(s) => {
                let bytes = string_payload(s, &mut stream)?;
                payload.extend_from_slice(&bytes);
            }
            Value::Bool(b) => payload.push(u8::from(*b) ^ stream.next()),
            Value::Number(n) => {
                // f64 via the reference float path: reuse the mantissaä½Ž byte
                // walk â€” smoke marks it with the signal name's first byte.
                payload.push(name.as_bytes()[0] ^ stream.next());
                let bits = n.0.to_bits();
                for shift in [0, 8, 16, 24] {
                    payload.push(((bits >> shift) & 0xff) as u8 ^ stream.next());
                }
            }
            Value::Bytes(b) => {
                for byte in b {
                    payload.push(*byte ^ stream.next());
                }
            }
            Value::Undefined | Value::Null => payload.push(stream.next()),
        }
    }

    // Chunk to the layout's a-size with the 4-byte big-endian length prefix.
    let mut a = Vec::new();
    let chunk_capacity = 48;
    let header = format!("AAAA{}", payload.len());
    let mut current = header.into_bytes();
    for byte in &payload {
        current.push(*byte);
        if current.len() >= chunk_capacity {
            a.push(String::from_utf8_lossy(&current).into_owned());
            current.clear();
        }
    }
    if !current.is_empty() {
        a.push(String::from_utf8_lossy(&current).into_owned());
    }

    Ok(HeaderBundle {
        a,
        b: Some(format!("{:x}", seeds[0] ^ seeds[1].rotate_left(17))),
        c: Some(String::new()),
        d: Some(String::new()),
        f: Some("ab".into()),
        z: Some(String::new()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::Synthesizer;

    fn device() -> crate::device::DeviceProfile {
        crate::device::DeviceProfile::win11_chrome()
    }

    #[test]
    fn smoke_encode_produces_parseable_bundle() {
        let signals = Synthesizer::new(device()).bootstrap(1).unwrap();
        let bundle = encode_smoke("x-example-", &signals, [19, 23]).unwrap();
        assert!(!bundle.a.is_empty());
        assert!(bundle.a[0].starts_with("AAAA"));
        let headers = bundle.http_headers("x-example-");
        assert!(headers.iter().any(|(n, _)| n == "x-example-a"));
        assert!(headers.iter().any(|(n, _)| n == "x-example-b"));
    }

    #[test]
    fn empty_signals_fail_loud() {
        let empty = NamedSignals::new();
        assert!(encode_smoke("x-", &empty, [1, 2]).is_err());
    }

    #[test]
    fn smoke_encode_is_deterministic_per_seed() {
        let signals = Synthesizer::new(device()).bootstrap(9).unwrap();
        let one = encode_smoke("x-", &signals, [19, 23]).unwrap();
        let two = encode_smoke("x-", &signals, [19, 23]).unwrap();
        assert_eq!(one, two);
        let other = encode_smoke("x-", &signals, [19, 24]).unwrap();
        assert_ne!(one.a, other.a, "seed changes the mask stream");
    }
}
