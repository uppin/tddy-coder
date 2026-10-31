//! A tool's `home:` section — the `$HOME` it runs with inside a jail.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-claude-cli-jail-from-spec.md` (`#agent-tools 5/12`).
//!
//! ```yaml
//! home: { kind: persistent, key: claude }   # <tddyhome>/homes/claude, mounted rw, kept across sessions
//! # home: { kind: session }                 # a fresh per-session home under the jail's scratch
//! ```
//!
//! A missing section is a session home. Lowering is `tddy-sandbox-recipes`' `lower_home`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum HomeSection {
    /// `<tddyhome>/homes/<key>`: one directory per key, shared by every session of every tool that
    /// names it, mounted read-write so what the tool writes there (refreshed credentials, history)
    /// survives the session.
    Persistent { key: String },
    /// A home that lives and dies with the session.
    #[default]
    Session,
}
