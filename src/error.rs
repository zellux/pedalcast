use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io;

#[derive(Debug)]
pub enum PedalcastError {
    Adapter(String),
    Config(String),
    Io {
        path: String,
        source: io::Error,
    },
    #[allow(dead_code)]
    Runtime(String),
    SuccessExit,
    Usage(String),
}

impl PedalcastError {
    pub fn adapter(message: impl Into<String>) -> Self {
        Self::Adapter(message.into())
    }

    pub fn config(message: impl Into<String>) -> Self {
        Self::Config(message.into())
    }

    #[allow(dead_code)]
    pub fn runtime(message: impl Into<String>) -> Self {
        Self::Runtime(message.into())
    }

    pub fn success_exit() -> Self {
        Self::SuccessExit
    }

    pub fn usage(message: impl Into<String>) -> Self {
        Self::Usage(message.into())
    }

    pub fn exit_code(&self) -> u8 {
        match self {
            Self::SuccessExit => 0,
            Self::Usage(_) | Self::Config(_) => 64,
            Self::Adapter(_) => 69,
            Self::Io { .. } | Self::Runtime(_) => 70,
        }
    }
}

impl Display for PedalcastError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Adapter(message)
            | Self::Config(message)
            | Self::Runtime(message)
            | Self::Usage(message) => formatter.write_str(message),
            Self::Io { path, source } => write!(formatter, "{path}: {source}"),
            Self::SuccessExit => formatter.write_str(""),
        }
    }
}

impl Error for PedalcastError {}
