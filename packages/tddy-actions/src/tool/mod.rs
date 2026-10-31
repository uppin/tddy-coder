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
//!
//! `#agent-tools 2/12` adds how a tool is **run** ([`RunSection`]: an argv template rendered by
//! [`render_argv`], static env, a transport) and open, owner-validated **extensions**
//! ([`ExtensionOwner`]) — `tddy-coding-agent-specs` owns `coding_agent`, so this crate stays
//! agent-agnostic.
//!
//! `#agent-tools 3/12` adds which hosts a tool may reach ([`NetworkSection`]); the jail's shim and
//! host relay enforce the union of a tool set's declarations.
//!
//! `#agent-tools 4/12` lets a tool's binary be a `tddy-build` target's output ([`TargetBinary`]),
//! answered by [`ToolHost::target_output`] on a host that knows a build graph.
//!
//! `#agent-tools 5/12` adds what a tool needs from a jail beyond its binary: its home
//! ([`HomeSection`]), the files placed there and the env pointing at jail paths ([`FileItem`]),
//! out-of-band secrets ([`SecretItem`]) and non-file allows ([`PolicySection`]). Like `network:`,
//! these are generic — any tool may declare them, and `tddy-sandbox-recipes` lowers them.
//!
//! `#agent-tools 11/12` adds the ACP transport ([`Transport::Acp`]) and a binary found beside a
//! dep's resolved binary ([`ToolBinary::sibling_of`], [`resolve_sibling`]).

mod def;
mod error;
mod extensions;
mod files;
mod home;
mod network;
mod policy;
mod registry;
mod resolve;
mod run;
mod secrets;
mod template;

pub use def::{TargetBinary, ToolBinary, ToolDef, TOOL_SCHEMA_VERSION};
pub use error::ToolError;
pub use extensions::ExtensionOwner;
pub use files::FileItem;
pub use home::HomeSection;
pub use network::{Egress, HostPattern, HostPatternError, NetworkSection, DEFAULT_EGRESS_PORT};
pub use policy::PolicySection;
pub use registry::{ToolRegistry, BUILTIN_SUPPORT_TOOLS, BUILTIN_TOOLS};
pub use resolve::{resolve_sibling, ProcessToolHost, ResolvedTool, ToolHost};
pub use run::{ArgvItem, RunSection, Transport};
pub use secrets::{RouteGuard, SecretItem, SecretItemSource};
pub use template::{render_argv, TemplateParams};
