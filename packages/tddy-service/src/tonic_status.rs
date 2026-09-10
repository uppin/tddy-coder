//! `Status` conversion between the transport-agnostic `tddy-rpc` flavor and tonic 0.12.
//!
//! Every adapter that serves a `tddy-rpc` service implementation over tonic gRPC — hand-written in
//! `tddy-daemon`, or emitted by `tddy-codegen`'s `generate_tonic_adapter` — has to map a refusal
//! across the boundary. They all convert through this one pair so a given refusal cannot reach two
//! transports as two different gRPC codes.
//!
//! `tddy-rpc` carries its own `From<Status> for tonic::Status`, but pins **tonic 0.11** while this
//! crate, `tddy-terminal-rpc` and `tddy-daemon` are all on **tonic 0.12** — two distinct types
//! named `tonic::Status`, so that impl cannot be used here.
//!
//! It lives in `tddy-service` rather than in a consumer because generated adapters land in the
//! `OUT_DIR` of `tddy-service` and `tddy-terminal-rpc`, and neither may depend on `tddy-daemon`:
//! the dependency runs the other way.

/// Convert a tddy-rpc `Status` into a tonic `Status` (tonic 0.12).
/// Public because the host and worktree adapters convert the same way: three adapters wrapping
/// three services over the same transport must map a refusal to the same tonic code, and three
/// copies of this match is three chances for one of them to drift.
pub fn to_tonic_status(status: tddy_rpc::Status) -> tonic::Status {
    let code = match status.code() {
        tddy_rpc::Code::Ok => tonic::Code::Ok,
        tddy_rpc::Code::Cancelled => tonic::Code::Cancelled,
        tddy_rpc::Code::Unknown => tonic::Code::Unknown,
        tddy_rpc::Code::InvalidArgument => tonic::Code::InvalidArgument,
        tddy_rpc::Code::DeadlineExceeded => tonic::Code::DeadlineExceeded,
        tddy_rpc::Code::NotFound => tonic::Code::NotFound,
        tddy_rpc::Code::AlreadyExists => tonic::Code::AlreadyExists,
        tddy_rpc::Code::PermissionDenied => tonic::Code::PermissionDenied,
        tddy_rpc::Code::ResourceExhausted => tonic::Code::ResourceExhausted,
        tddy_rpc::Code::FailedPrecondition => tonic::Code::FailedPrecondition,
        tddy_rpc::Code::Aborted => tonic::Code::Aborted,
        tddy_rpc::Code::OutOfRange => tonic::Code::OutOfRange,
        tddy_rpc::Code::Unimplemented => tonic::Code::Unimplemented,
        tddy_rpc::Code::Internal => tonic::Code::Internal,
        tddy_rpc::Code::Unavailable => tonic::Code::Unavailable,
        tddy_rpc::Code::DataLoss => tonic::Code::DataLoss,
        tddy_rpc::Code::Unauthenticated => tonic::Code::Unauthenticated,
    };
    tonic::Status::new(code, status.message().to_string())
}

/// Convert a tonic `Status` (tonic 0.12) into a tddy-rpc `Status`.
///
/// Public because handlers can produce a tonic `Status` directly — a refusal raised by a helper
/// shared with a tonic-typed caller — and the tddy-rpc `tonic` feature (which would provide the
/// `From` impls) is not enabled for the crates that need this. A bidirectional adapter needs it in
/// this direction as well: the inbound half of the stream carries tonic refusals into a handler
/// written against tddy-rpc.
pub fn to_rpc_status(status: tonic::Status) -> tddy_rpc::Status {
    let code = match status.code() {
        tonic::Code::Ok => tddy_rpc::Code::Ok,
        tonic::Code::Cancelled => tddy_rpc::Code::Cancelled,
        tonic::Code::Unknown => tddy_rpc::Code::Unknown,
        tonic::Code::InvalidArgument => tddy_rpc::Code::InvalidArgument,
        tonic::Code::DeadlineExceeded => tddy_rpc::Code::DeadlineExceeded,
        tonic::Code::NotFound => tddy_rpc::Code::NotFound,
        tonic::Code::AlreadyExists => tddy_rpc::Code::AlreadyExists,
        tonic::Code::PermissionDenied => tddy_rpc::Code::PermissionDenied,
        tonic::Code::ResourceExhausted => tddy_rpc::Code::ResourceExhausted,
        tonic::Code::FailedPrecondition => tddy_rpc::Code::FailedPrecondition,
        tonic::Code::Aborted => tddy_rpc::Code::Aborted,
        tonic::Code::OutOfRange => tddy_rpc::Code::OutOfRange,
        tonic::Code::Unimplemented => tddy_rpc::Code::Unimplemented,
        tonic::Code::Internal => tddy_rpc::Code::Internal,
        tonic::Code::Unavailable => tddy_rpc::Code::Unavailable,
        tonic::Code::DataLoss => tddy_rpc::Code::DataLoss,
        tonic::Code::Unauthenticated => tddy_rpc::Code::Unauthenticated,
    };
    tddy_rpc::Status {
        code,
        message: status.message().to_string(),
    }
}
