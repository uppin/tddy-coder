//! Language backends. Each one turns a refactoring intent into a concrete multi-file edit by
//! asking that language's own refactoring engine.

pub mod code_action;
pub mod lsp_bridge;
pub mod rust;
pub mod server;

pub use lsp_bridge::LspClientBridge;
pub use rust::RustBackend;
pub use server::ServerBackend;
