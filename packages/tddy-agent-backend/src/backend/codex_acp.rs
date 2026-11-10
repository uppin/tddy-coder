//! The Codex ACP login's OAuth helpers, read by the `codex-acp-login` hooks adapter
//! (`crate::adapters::CodexAcpLogin`). The bespoke `CodexAcpBackend` that owned them is
//! deleted: every ACP agent runs on [`super::SpecAcpBackend`] from its spec.

pub mod oauth;
