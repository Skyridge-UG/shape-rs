//! `schema-dump` â€” pinned-layout intelligence for agents.
//!
//! Reads a shape-engine seed layout JSON (e.g. the pinned corpus) and prints
//! the machine-readable contract a Shape kernel change alters: signal slots,
//! cipher geometry, header policy, keysets. Run this FIRST after any kernel
//! change and diff against the previous dump â€” the diff IS the change list.

use shape::layout::SeedLayout;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: schema-dump <seed-layout.json>");
        std::process::exit(2);
    });
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("read {path}: {e}");
            std::process::exit(2);
        }
    };
    let layout = match SeedLayout::from_json(&bytes) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("parse: {e}");
            std::process::exit(2);
        }
    };

    let doc = serde_json::json!({
        "source": { "sha256": layout.source_sha256(), "bytes": layout.source_bytes() },
        "session": {
            "transaction_token": layout.session.transaction_token,
            "alphabet": layout.session.alphabet,
            "keys": layout.session.keys,
        },
        "cipher": {
            "ivs": layout.cipher.ivs,
            "counter": layout.cipher.counter,
            "double_rounds": layout.cipher.double_rounds,
        },
        "header": {
            "name": layout.header.name,
            "prefix": layout.header.prefix,
            "chunk_size": layout.header.chunk_size,
            "channels": {
                "b": layout.header.b,
                "c": layout.header.c,
                "d": layout.header.d,
                "f": layout.header.f,
                "z": layout.header.z,
            },
        },
        "signal_slots": layout.signal_slots(),
    });

    println!("{}", serde_json::to_string_pretty(&doc).unwrap_or_default());
}
