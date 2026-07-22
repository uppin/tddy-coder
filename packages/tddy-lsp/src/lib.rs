//! Reusable language-server (LSP) support.
//!
//! Runs a language server (rust-analyzer first, any LSP thereafter) as a long-running
//! [`tddy_task`] task, reused across build targets and keyed by
//! [`LspKey`](registry::LspKey) = (workspace root, language). The server for a target is
//! chosen from the target's type via [`language_for_target_type`](allowlist::language_for_target_type),
//! gated by an [`LspAllowList`](allowlist::LspAllowList). This crate holds only the LSP
//! mechanics — it has no dependency on `tddy-build` or `tddy-core`; the binaries bind the
//! domain types to this registry (mirroring the `BuildExecutor` extension pattern).

pub mod allowlist;
pub mod client;
pub mod error;
pub mod protocol;
pub mod registry;
pub mod server_body;

pub use allowlist::{language_for_target_type, Language, LaunchSpec, LspAllowList};
pub use client::{Diagnostic, Location, LspClient, Position, Range, SymbolInfo};
pub use error::LspError;
pub use registry::{DocumentSource, LspKey, LspRegistry, LspService};
pub use server_body::LspServerBody;
