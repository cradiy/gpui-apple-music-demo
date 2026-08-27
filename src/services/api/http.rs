use std::fmt;

#[derive(Debug)]
pub enum ApiError {
    AuthenticationRequired,
    Session(String),
    Network(String),
    Http {
        status: u16,
        code: String,
        message: String,
        request_id: Option<String>,
    },
    InvalidResponse(String),
}

impl ApiError {
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Http { status, .. } => Some(*status),
            _ => None,
        }
    }

    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Http { code, .. } => Some(code),
            _ => None,
        }
    }

    pub fn user_message(&self) -> String {
        match self {
            Self::AuthenticationRequired => "The local player is not ready".to_owned(),
            Self::Session(_) => "The local player state could not be saved".to_owned(),
            Self::Network(error) | Self::InvalidResponse(error) => error.clone(),
            Self::Http { message, .. } => message.clone(),
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthenticationRequired => write!(f, "local player state is unavailable"),
            Self::Session(error) => write!(f, "local state error: {error}"),
            Self::Network(error) => write!(f, "local I/O error: {error}"),
            Self::Http {
                status,
                code,
                message,
                request_id,
            } => {
                write!(f, "local resource {status} ({code}): {message}")?;
                if let Some(request_id) = request_id {
                    write!(f, " [{request_id}]")?;
                }
                Ok(())
            }
            Self::InvalidResponse(error) => write!(f, "local data error: {error}"),
        }
    }
}

impl std::error::Error for ApiError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ByteRange {
    From(u64),
    Inclusive { start: u64, end: u64 },
    Suffix(u64),
}

impl fmt::Display for ByteRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::From(start) => write!(f, "bytes={start}-"),
            Self::Inclusive { start, end } => write!(f, "bytes={start}-{end}"),
            Self::Suffix(length) => write!(f, "bytes=-{length}"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BinaryMetadata {
    pub status: u16,
    pub content_type: String,
    pub content_length: Option<u64>,
    pub content_range: Option<String>,
    pub accept_ranges: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BinaryResponse {
    pub metadata: BinaryMetadata,
    pub bytes: Vec<u8>,
}
