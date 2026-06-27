use std::fmt;

/// Errors from sandbox operations.
#[derive(Debug)]
pub enum SandboxError {
    /// The platform does not support sandboxes yet.
    Unsupported { platform: String, message: String },
    /// I/O or spawn failure.
    Io(String),
    /// Invalid sandbox specification.
    InvalidSpec(String),
}

impl fmt::Display for SandboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported { platform, message } => {
                write!(f, "sandbox unsupported on {platform}: {message}")
            }
            Self::Io(msg) => write!(f, "sandbox I/O error: {msg}"),
            Self::InvalidSpec(msg) => write!(f, "invalid sandbox spec: {msg}"),
        }
    }
}

impl std::error::Error for SandboxError {}
