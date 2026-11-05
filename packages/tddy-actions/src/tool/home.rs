//! A tool's `home:` section — the `$HOME` it runs with inside a jail.
//!
//! Feature: `docs/ft/daemon/claude-cli-session.md` § The jail from the `claude` tool definition (`#agent-tools 5/12`).
//!
//! ```yaml
//! home: { kind: persistent, key: claude }   # <tddyhome>/homes/claude, mounted rw, kept across sessions
//! # home: { kind: session }                 # a fresh per-session home under the jail's scratch
//! # home: { kind: path, path: /srv/claude }  # an operator's own directory, mounted rw (`#agent-tools 19/19`)
//! ```
//!
//! A missing section is a session home. Lowering is `tddy-sandbox-recipes`' `lower_home`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum HomeSection {
    /// `<tddyhome>/homes/<key>`: one directory per key, shared by every session of every tool that
    /// names it, mounted read-write so what the tool writes there (refreshed credentials, history)
    /// survives the session.
    Persistent { key: String },
    /// An absolute directory the operator names, mounted read-write and kept across sessions — what
    /// daemon.yaml `claude_cli.claude_home_dir` was before `#agent-tools 19/19` removed it. A
    /// relative path is refused at load.
    Path { path: PathBuf },
    /// A home that lives and dies with the session.
    #[default]
    Session,
}
