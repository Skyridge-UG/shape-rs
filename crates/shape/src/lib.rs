//! shape-rs core: session, device, seed layout, signal synthesis, wire codecs.
//!
//! Own API, house style (reese84-rs parity). The vendored reference engine is
//! an implementation detail behind [`pin`]; everything a caller touches lives
//! here as semantic, documented types.

pub mod bandai;
pub mod catalog;
pub mod seedfamily;

/// One-call engine seal (engine-gated; see `fullcircle`).
#[cfg(feature = "engine")]
pub fn engine_seal(
    session: &crate::session::Session,
    family: &crate::seedfamily::FamilyHeaders,
    request_seed: u32,
) -> crate::error::Result<String> {
    let program = crate::pin::compose_reference_program_pub();
    let pinned = crate::pin::pin(&crate::pin::PinInput {
        source: &program,
        session,
    })?;
    let mut signals = crate::signal::NamedSignals::new();
    signals
        .set("storage_access_api_presence", crate::signal::Value::Bool(true))
        .set(
            "device_memory",
            crate::signal::Value::Number(crate::signal::Number(9.0)),
        );
    let seeds = [
        request_seed,
        u32::from_le_bytes([
            family.b.as_bytes().first().copied().unwrap_or(0),
            family.b.as_bytes().get(1).copied().unwrap_or(0),
            family.b.as_bytes().get(2).copied().unwrap_or(0),
            family.b.as_bytes().get(3).copied().unwrap_or(0),
        ]),
    ];
    let bundle = crate::encode::encode_headers(&pinned.engine, &signals, seeds)?;
    Ok(bundle.a.concat())
}

pub mod device;
pub mod encode;
pub mod error;
pub mod layout;
pub mod pin;
pub mod session;
pub mod signal;
pub mod synth;
pub mod wire;

/// Convenience prelude for server/gen binaries.
pub mod prelude {
    pub use crate::device::DeviceProfile;
    pub use crate::error::{Error, Result};
    pub use crate::session::Session;
    pub use crate::signal::{NamedSignals, Value};
    pub use crate::synth::Synthesizer;
}

// Re-exports the bins address directly (stable paths, no prelude needed).
pub use crate::device::DeviceProfile;
