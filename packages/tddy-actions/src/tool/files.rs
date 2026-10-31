//! A tool's `files:` section — what is placed in its home before it starts, and the env that points
//! it at jail paths.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-claude-cli-jail-from-spec.md` (`#agent-tools 5/12`).
//!
//! ```yaml
//! files:
//!   - { kind: seed-once, from_host: .claude/.credentials.json, to: .claude/.credentials.json }
//!   - { kind: mirror, from_host: .local/bin/claude, to: .local/bin/claude }
//!   - { kind: overlay, env: CLAUDE_CODE_TMPDIR, value: "{tmp}" }
//! ```
//!
//! `from_host` is relative to the host user's home and `to` to the tool's home: a tool file never
//! names an absolute path of either side. Lowering is `tddy-sandbox-recipes`' `lower_files`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum FileItem {
    /// Copy the host file into the home **only when the home lacks it** — a persistent home may
    /// hold a newer copy the tool refreshed itself (an OAuth token), which the host's must not
    /// clobber. A host file that does not exist seeds nothing.
    SeedOnce { from_host: PathBuf, to: PathBuf },
    /// A symlink at `to` in the home pointing at the host path: the tool finds its own install
    /// where it expects it, without a copy.
    Mirror { from_host: PathBuf, to: PathBuf },
    /// Set `env` on the tool's process. `value` may name `{home}` (the tool's home) and `{tmp}`
    /// (the jail's temporary directory); every other character is literal.
    Overlay { env: String, value: String },
}
