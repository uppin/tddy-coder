//! A warm rust-analyzer index, served as `code_index.CodeIndexService`.
//!
//! This crate owns `proto/code_index.proto`, serves it, and publishes its coordinate. It exists
//! because `tddy-tools restructure` builds a `TaskRegistry` and an `LspRegistry` per invocation and
//! drops both on exit, so every run pays a full crate-graph load — six to ten minutes on a
//! workspace this size, once per retry of an iterative carve.
//!
//! The index is held per workspace root, in the shape `tddy_lsp::LspRegistry` already provides:
//! keyed by `(root, language)`, lazily started, reaped when idle, respawned when its task dies. One
//! process therefore serves several worktrees, and every request names the root it means.
//!
//! Two lifetimes, one implementation. With no transport argument the binary runs a single operation
//! against the generated service trait **in process** — prost structs in, prost structs out, no
//! encode/decode — and exits with a status. With `--grpc` and/or `--stdio` it serves that same
//! implementation and stays alive. There is one code path, not two that must be kept in step.

pub mod analyze;
mod apply;
pub mod index;
pub mod operations;
pub mod queries;
pub mod service;
pub mod status;

pub use service::{build_code_index_entry, CodeIndexPorts, CodeIndexServiceImpl, EventStream};

pub mod proto {
    /// The canonical message types, the `tddy-rpc` service trait and server, and the tonic adapter
    /// that delegates one implementation of that trait to the gRPC server trait below.
    pub mod code_index {
        include!(concat!(env!("OUT_DIR"), "/code_index.rs"));
    }

    /// The tonic gRPC server and client, over the same message types via `extern_path`.
    pub mod tonic_code_index {
        #![allow(unused_imports, clippy::all)]
        include!(concat!(env!("OUT_DIR"), "/tonic_code_index/code_index.rs"));
    }
}

/// The coordinate a client addresses this service at.
///
/// It lives in this crate rather than in `tddy-service` because this crate owns
/// `code_index.proto`, is the only crate that serves it, and is a dependency of every caller that
/// addresses it — the rule `tddy-terminal-rpc` states for `TERMINAL_SESSION_SERVICE`. Only
/// coordinates whose served and addressed ends sit in different crates belong in the shared
/// catalog.
pub const CODE_INDEX_SERVICE: &str = "code_index.CodeIndexService";

/// The proto's file-descriptor set, so a host can merge this service into gRPC reflection without
/// re-parsing the schema.
pub static CODE_INDEX_DESCRIPTOR_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/code_index_descriptors.bin"));
