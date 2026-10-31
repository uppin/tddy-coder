//! A tool's `run:` section — how tddy launches it: an argv template, static env and a transport.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-claude-cli-from-spec.md` (`#agent-tools 2/12`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// ```yaml
/// run:
///   transport: pty
///   argv: [ "{binary}", "--model", "{model}",
///           { if: resume, then: ["--resume", "{session_id}"], else: ["--session-id", "{session_id}"] } ]
///   env: { SOME_VAR: "1" }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunSection {
    /// How the launched process is attached. Required: a launcher for one transport refuses the
    /// other, so a file must say which it is.
    pub transport: Transport,
    /// The argv template, rendered by [`super::render_argv`]. `{binary}` is always bound to the
    /// resolved binary.
    pub argv: Vec<ArgvItem>,
    /// Set on the process on top of what the launcher passes. Values are literal.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

/// How a launched process is attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    /// An interactive terminal.
    Pty,
    /// Piped stdin/stdout (a headless agent).
    Stdio,
    /// The Agent Client Protocol — JSON-RPC over piped stdin/stdout, driven by tddy's ACP driver
    /// (`#agent-tools 11/12`: `claude-agent-acp`, `codex-acp`, `opencode acp`).
    Acp,
}

/// One argv template item: a literal (which may hold `{param}` placeholders), or a segment rendered
/// only when a parameter is set.
///
/// A structured template, not a string mini-language: it is reviewable, serde-checked, and it
/// cannot inject shell — every rendered item is exactly one argv entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgvItem {
    /// One argv entry. `{name}` is replaced by the parameter's value; an unbound name is an error.
    Lit(String),
    /// `then` when the parameter `if_` is set (bound to a non-empty value), `else_` otherwise.
    When {
        #[serde(rename = "if")]
        if_: String,
        then: Vec<ArgvItem>,
        #[serde(rename = "else", default, skip_serializing_if = "Vec::is_empty")]
        else_: Vec<ArgvItem>,
    },
    /// The list parameter `splice` expanded in place, one argv entry per element (a launch's
    /// rendered slot flags). An unbound list is an error, as an unbound placeholder is.
    Splice { splice: String },
}
