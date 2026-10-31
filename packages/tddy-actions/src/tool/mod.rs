//! Declared tools — a tool is a YAML definition of an executable and what it needs.
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-declared-tools.md` (`#agent-tools 1/12`).
//!
//! The Bazel split: a **tool** ([`ToolDef`]) is *what an executable is* — how its binary is found
//! on the host, which other tools it depends on, and which host paths it needs readable. An
//! **action** ([`crate::ActionSpec`]) is one invocation. This module knows nothing about sandboxes:
//! it loads definitions ([`ToolRegistry`]) and resolves them against a host ([`ToolHost`]) into
//! [`ResolvedTool`]s. Lowering a resolution into jail grants is `tddy-sandbox-recipes`'
//! `tool_grants`, so the generic sandbox crate never learns this vocabulary.

mod def;
mod error;
mod registry;
mod resolve;

pub use def::{ToolBinary, ToolDef, TOOL_SCHEMA_VERSION};
pub use error::ToolError;
pub use registry::{ToolRegistry, BUILTIN_TOOLS};
pub use resolve::{ProcessToolHost, ResolvedTool, ToolHost};
