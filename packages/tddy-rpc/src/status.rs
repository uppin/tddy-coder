//! gRPC-like Status type for RPC errors.

use std::fmt;

/// RPC error status (gRPC-like).
#[derive(Debug, Clone)]
pub struct Status {
    pub code: Code,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    Ok,
    Cancelled,
    Unknown,
    InvalidArgument,
    DeadlineExceeded,
    NotFound,
    AlreadyExists,
    PermissionDenied,
    ResourceExhausted,
    FailedPrecondition,
    Aborted,
    OutOfRange,
    Unimplemented,
    Internal,
    Unavailable,
    DataLoss,
    Unauthenticated,
}

impl Status {
    pub fn ok() -> Self {
        Self {
            code: Code::Ok,
            message: String::new(),
        }
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self {
            code: Code::NotFound,
            message: msg.into(),
        }
    }

    pub fn invalid_argument(msg: impl Into<String>) -> Self {
        Self {
            code: Code::InvalidArgument,
            message: msg.into(),
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            code: Code::Internal,
            message: msg.into(),
        }
    }

    pub fn unimplemented(msg: impl Into<String>) -> Self {
        Self {
            code: Code::Unimplemented,
            message: msg.into(),
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl Code {
    pub fn as_str(&self) -> &'static str {
        match self {
            Code::Ok => "OK",
            Code::Cancelled => "CANCELLED",
            Code::Unknown => "UNKNOWN",
            Code::InvalidArgument => "INVALID_ARGUMENT",
            Code::DeadlineExceeded => "DEADLINE_EXCEEDED",
            Code::NotFound => "NOT_FOUND",
            Code::AlreadyExists => "ALREADY_EXISTS",
            Code::PermissionDenied => "PERMISSION_DENIED",
            Code::ResourceExhausted => "RESOURCE_EXHAUSTED",
            Code::FailedPrecondition => "FAILED_PRECONDITION",
            Code::Aborted => "ABORTED",
            Code::OutOfRange => "OUT_OF_RANGE",
            Code::Unimplemented => "UNIMPLEMENTED",
            Code::Internal => "INTERNAL",
            Code::Unavailable => "UNAVAILABLE",
            Code::DataLoss => "DATA_LOSS",
            Code::Unauthenticated => "UNAUTHENTICATED",
        }
    }
}

#[cfg(feature = "tonic")]
mod tonic_impl {
    use super::*;

    impl From<Status> for tonic::Status {
        fn from(s: Status) -> Self {
            let code = match s.code {
                Code::Ok => tonic::Code::Ok,
                Code::Cancelled => tonic::Code::Cancelled,
                Code::Unknown => tonic::Code::Unknown,
                Code::InvalidArgument => tonic::Code::InvalidArgument,
                Code::DeadlineExceeded => tonic::Code::DeadlineExceeded,
                Code::NotFound => tonic::Code::NotFound,
                Code::AlreadyExists => tonic::Code::AlreadyExists,
                Code::PermissionDenied => tonic::Code::PermissionDenied,
                Code::ResourceExhausted => tonic::Code::ResourceExhausted,
                Code::FailedPrecondition => tonic::Code::FailedPrecondition,
                Code::Aborted => tonic::Code::Aborted,
                Code::OutOfRange => tonic::Code::OutOfRange,
                Code::Unimplemented => tonic::Code::Unimplemented,
                Code::Internal => tonic::Code::Internal,
                Code::Unavailable => tonic::Code::Unavailable,
                Code::DataLoss => tonic::Code::DataLoss,
                Code::Unauthenticated => tonic::Code::Unauthenticated,
            };
            tonic::Status::new(code, s.message)
        }
    }

    impl From<tonic::Status> for Status {
        fn from(s: tonic::Status) -> Self {
            let code = match s.code() {
                tonic::Code::Ok => Code::Ok,
                tonic::Code::Cancelled => Code::Cancelled,
                tonic::Code::Unknown => Code::Unknown,
                tonic::Code::InvalidArgument => Code::InvalidArgument,
                tonic::Code::DeadlineExceeded => Code::DeadlineExceeded,
                tonic::Code::NotFound => Code::NotFound,
                tonic::Code::AlreadyExists => Code::AlreadyExists,
                tonic::Code::PermissionDenied => Code::PermissionDenied,
                tonic::Code::ResourceExhausted => Code::ResourceExhausted,
                tonic::Code::FailedPrecondition => Code::FailedPrecondition,
                tonic::Code::Aborted => Code::Aborted,
                tonic::Code::OutOfRange => Code::OutOfRange,
                tonic::Code::Unimplemented => Code::Unimplemented,
                tonic::Code::Internal => Code::Internal,
                tonic::Code::Unavailable => Code::Unavailable,
                tonic::Code::DataLoss => Code::DataLoss,
                tonic::Code::Unauthenticated => Code::Unauthenticated,
            };
            Status {
                code,
                message: s.message().to_string(),
            }
        }
    }
}
