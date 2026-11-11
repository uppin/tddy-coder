//! Per-conversation token accounting — the record shape and summary renderer, which live in
//! [`tddy_usage::token_accounting`] beside the engine that produces them. Re-exported here so every
//! `tddy_agent_backend::token_accounting` / `tddy_core::token_accounting` path keeps resolving.
//!
//! Feature: docs/ft/coder/session-token-accounting.md

pub use tddy_usage::token_accounting::*;
