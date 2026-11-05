use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    Device(String),
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Protocol(String),
    #[error("{0}")]
    Verification(String),
    #[error("{0}")]
    Internal(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Time(String),
}

impl Error {
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Usage(_) => 2,
            Self::Device(_) => 3,
            Self::Validation(_) => 4,
            Self::Protocol(_) => 5,
            Self::Verification(_) => 6,
            Self::Internal(_) | Self::Io(_) | Self::Json(_) | Self::Time(_) => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exit(pub u8);
