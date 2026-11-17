//! Pin: raw kernel JS + session -> [`SeedLayout`].
//!
//! With the `engine` feature, delegates the AST/VM analysis to the vendored
//! reference engine and validates its `PinnedSeed.layout` as our layout.
//! Without it, artifact validation only (the server consumes pre-pinned
//! layouts; only the `shapepin` tool needs the engine).

use crate::error::Result;
#[cfg(feature = "engine")]
use crate::error::Error;
use crate::layout::SeedLayout;
use crate::session::Session;

/// Facts `pin` needs: the raw kernel JavaScript and the session values.
pub struct PinInput<'a> {
    pub source: &'a str,
    pub session: &'a Session,
}

/// Pin via the vendored engine: returns the engine layout verbatim (the
/// encode/decode codec needs its full report grammar) plus our validated
/// mirror for the server path.
#[cfg(feature = "engine")]
pub struct Pinned {
    /// Engine layout: pass to `encode::encode_headers` / `decode_headers`.
    pub engine: shape_engine::signals::payload::extract::SeedLayout,
    /// Our validated mirror (subset) for persistence/inspection.
    pub mirror: SeedLayout,
}

#[cfg(feature = "engine")]
pub fn pin(input: &PinInput<'_>) -> Result<Pinned> {
    let engine_session = shape_engine::signals::payload::extract::Session {
        transaction_token: input.session.transaction_token.clone(),
        alphabet: input.session.alphabet.clone(),
        shuffle_key: input.session.shuffle_key.clone(),
        keys: input.session.keys,
        nonce: None,
    };

    let pinned = shape_engine::signals::payload::extract::pin(input.source, &engine_session)
        .map_err(|e| Error::Pin(format!("engine pin: {e}")))?;

    let layout = pinned.layout;
    let mirror = mirror_of(&layout);
    mirror.validate()?;
    Ok(Pinned {
        engine: layout,
        mirror,
    })
}

#[cfg(feature = "engine")]
fn mirror_of(layout: &shape_engine::signals::payload::extract::SeedLayout) -> SeedLayout {
    SeedLayout {
        format: layout.format.clone(),
        session: crate::layout::LayoutSession {
            transaction_token: layout.session.transaction_token.clone(),
            alphabet: layout.session.alphabet.clone(),
            shuffle_key: layout.session.shuffle_key.clone(),
            keys: layout.session.keys,
        },
        cipher: crate::layout::CipherLayout {
            ivs: layout.cipher.ivs,
            counter: layout.cipher.counter,
            double_rounds: layout.cipher.double_rounds,
        },
        header: crate::layout::HeaderLayout {
            name: layout.header.name.clone(),
            prefix: layout.header.prefix.clone(),
            chunk_size: 1000,
            b: layout.header.b,
            c: layout.header.c.clone(),
            d: layout.header.d.clone(),
            f: layout.header.f,
            z: layout.header.z.clone(),
        },
        source: Some(crate::layout::SourceRef {
            sha256: layout.source.sha256.clone(),
            bytes: u64::try_from(layout.source.bytes).unwrap_or(u64::MAX),
        }),
        keysets: layout.keysets.clone(),
        report: serde_json::Value::Null,
    }
}

/// Engine-free path: validate an already-pinned artifact's JSON.
pub fn load_layout(json: &[u8]) -> Result<SeedLayout> {
    SeedLayout::from_json(json)
}

/// Public re-entry for integration tests (slot-diff tooling): the composed
/// reference program the e2e roundtrip pins.
#[cfg(feature = "engine")]
pub fn compose_reference_program_pub() -> String {
    compose_reference_program_impl()
}

/// The composer body (module level; shared by tests via the pub re-entry).
#[cfg(feature = "engine")]
fn compose_reference_program_impl() -> String {
    let template = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../.qwen/tmp/shape-research/tests/fixtures/signal-wire-roundtrip/program.js"
    ));
    let hash = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../.qwen/tmp/shape-research/src/signals/wire/reference/hash.js"
    ))
    .replace("local0(", "tokenHash(");
    let stream = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../.qwen/tmp/shape-research/src/signals/wire/reference/stream.js"
    ))
    .replace("local0(", "streamFactory(")
    .replace("local9", "tokenState");
    let number = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../.qwen/tmp/shape-research/src/signals/wire/reference/number.js"
    ))
    .replace("local0(", "integerWriter(");

    template
        .replace("__HASH__", &hash)
        .replace("__STREAM__", &stream)
        .replacen(
            "function main() {",
            &format!("function main() {{\n{number}"),
            1,
        )
        .replace(
            "function Session(token) {",
            "function Session(token) {\nlet sharedNumber; sharedNumber=integerWriter; const navigator={deviceMemory:9};",
        )
        .replace(
            "b[4]=9^mask();",
            "let callable; callable=sharedNumber; callable=callable(mask,navigator.deviceMemory,b,3); callable=0;",
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "engine")]
    #[test]
    fn pins_minimal_session_shape() {
        let session = Session {
            transaction_token: "ab".into(),
            alphabet: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
                .into(),
            shuffle_key: vec![],
            keys: [0; 8],
        };
        let program = compose_reference_program_pub();
        let pinned = pin(&PinInput {
            source: &program,
            session: &session,
        })
        .expect("engine pin of reference program");
        pinned.mirror.validate().unwrap();
        assert!(!pinned.mirror.source_sha256().is_empty());
    }

    #[test]
    fn load_layout_roundtrips_valid_json() {
        let layout = serde_json::json!({
            "format": crate::layout::SEED_LAYOUT_FORMAT,
            "session": {
                "transactionToken": "ab",
                "alphabet": "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
            },
            "cipher": { "ivs": [1, 2, 3, 4], "counter": 5, "doubleRounds": 1 },
            "header": { "name": "x-example-a", "prefix": "x-example-" },
            "sourceSha256": "aa",
            "sourceBytes": 1,
        });
        let loaded = load_layout(&serde_json::to_vec(&layout).unwrap()).unwrap();
        assert_eq!(loaded.header.name, "x-example-a");
    }
}
