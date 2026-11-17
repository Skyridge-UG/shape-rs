//! Crate error type: fail loud, wrap upstream causes with context.

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("session: {0}")]
    Session(String),

    #[error("device profile `{0}`: {1}")]
    Device(String, String),

    #[error("pin: {0}")]
    Pin(String),

    #[error("layout: {0}")]
    Layout(String),

    #[error("decode: {0}")]
    Decode(String),

    #[error("encode: {0}")]
    Encode(String),

    #[error("synthesis of signal `{signal}`: {message}")]
    Synthesis {
        signal: &'static str,
        message: String,
    },

    #[error("wire codec: {0}")]
    Wire(String),

    #[error("serde: {0}")]
    Serde(String),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

impl serde::ser::Error for Error {
    fn custom<T: std::fmt::Display>(message: T) -> Self {
        Self::Serde(message.to_string())
    }
}

impl serde::de::Error for Error {
    fn custom<T: std::fmt::Display>(message: T) -> Self {
        Self::Serde(message.to_string())
    }
}
