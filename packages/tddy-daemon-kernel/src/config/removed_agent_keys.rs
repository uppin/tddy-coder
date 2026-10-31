//! The daemon.yaml agent sections `#agent-tools 12/12` removes: `claude_cli:`, `cursor_cli:` and
//! `cli_harnesses:`.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-coding-agent-registry.md`. What they configured now
//! lives in the agent's tool file, `<tddyhome>/tools/<id>.yaml` — an override of the built-in spec.
//! A daemon.yaml that still carries one is refused at start-up, naming that file and where each of
//! its fields went, rather than ignored: a leftover section must not look like a setting that still
//! does something.
//!
//! Checked on the untyped document, before typed deserialization, like the obsolete `supervisor:`
//! block.

/// One removed section a daemon.yaml still carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedAgentKey {
    /// The section as written: `claude_cli`, `cursor_cli`, or `cli_harnesses.<session type>`.
    pub key: String,
    /// The id of the tool file that replaces it (`claude`, `cursor`, `opencode`).
    pub tool: String,
    /// Each field the section set, and the spec field it moved to (`binary_path` →
    /// `binary.command`), in the order the section wrote them.
    pub moved: Vec<(String, String)>,
}

impl std::fmt::Display for RemovedAgentKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let moved = self
            .moved
            .iter()
            .map(|(field, to)| format!("{field} → {to}"))
            .collect::<Vec<_>>()
            .join(", ");
        write!(
            f,
            "daemon.yaml `{}:` is removed: moved to <tddyhome>/tools/{}.yaml ({moved})",
            self.key, self.tool
        )
    }
}

/// A daemon.yaml that carries one or more removed agent sections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedAgentConfigError {
    /// Every removed section the file carries, `claude_cli` first, then `cursor_cli`, then each
    /// `cli_harnesses` entry in the order written.
    pub removed: Vec<RemovedAgentKey>,
}

impl std::fmt::Display for RemovedAgentConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let lines = self
            .removed
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        f.write_str(&lines)
    }
}

impl std::error::Error for RemovedAgentConfigError {}

/// `Err` naming every removed agent section `document` (a parsed daemon.yaml) carries, with the
/// tool file and spec field each of its fields moved to; `Ok` for a document that carries none.
///
/// The moves (PRD § Summary): `binary_path` → `binary.command`, `claude_home_dir` and
/// `cursor_home_dir` → `home.key`, `fullscreen` → `run.env`, `answer_wait_secs` →
/// `coding_agent.answer_wait_secs`. A `cli_harnesses.<session type>` entry names the tool its
/// session type runs (`opencode-cli` → `opencode`).
pub fn refuse_removed_agent_keys(
    document: &serde_yaml::Value,
) -> Result<(), RemovedAgentConfigError> {
    // TODO(agent-registry): implement.
    let _ = document;
    todo!("agent-registry: refuse_removed_agent_keys")
}
