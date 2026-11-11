//! The `session` extension: the session type a tool file declares, and how a session of it is
//! launched.
//!
//! Session types come only from declarations. A coding agent that runs as a session declares its
//! own (`claude.yaml`: `claude-cli`), and so does `tddy-coder` (`tool`), so neither Rust nor
//! TypeScript lists them. The one session type nothing declares is a session with no agent,
//! [`NO_AGENT_SESSION_TYPE`], which a tool file is refused for naming.

use serde::{Deserialize, Serialize};
use tddy_actions::tool::ExtensionOwner;
pub use tddy_workflow::{is_no_agent_session_type, NO_AGENT_SESSION_TYPE};

/// The `extensions:` key this module owns.
pub const SESSION_EXTENSION: &str = "session";

/// How a session of a declared type is launched: a closed set, each implemented in the launcher.
/// Dispatch matches on this, never on the session type's spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchKind {
    /// The tool is a coding agent whose `run:` section is a PTY: started, resumed and re-attached
    /// through the spec-driven CLI launch.
    AgentCli,
    /// `tddy-coder`: its binary is the start's `tool_path`, one of the daemon's `allowed_tools`, and
    /// it is spawned as a tool session over LiveKit.
    TddyCoder,
}

impl LaunchKind {
    /// The word a tool file declares this launch with.
    pub fn name(self) -> &'static str {
        match self {
            Self::AgentCli => "agent_cli",
            Self::TddyCoder => "tddy_coder",
        }
    }
}

impl std::fmt::Display for LaunchKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// `extensions.session`: the session type a tool declares, its launch and its label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionSection {
    /// The wire value a `StartSession` names to start a session of this tool, and what a session's
    /// `.session.yaml` records. Unique across the host's tool files.
    pub session_type: String,
    /// How a session of it is launched.
    pub launch: LaunchKind,
    /// What a picker calls the session type.
    pub label: String,
}

/// One session type a host declares: its section, and the id of the tool file that declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredSessionType {
    pub session_type: String,
    pub launch: LaunchKind,
    pub label: String,
    /// The tool file that declares it — for [`LaunchKind::AgentCli`], the coding agent it runs.
    pub tool_id: String,
}

/// `section` as a [`SessionSection`]: refused when it does not deserialize, when its session type
/// or label is blank, and when it declares [`NO_AGENT_SESSION_TYPE`].
pub(crate) fn parse_session_section(section: &serde_yaml::Value) -> Result<SessionSection, String> {
    let parsed: SessionSection =
        serde_yaml::from_value(section.clone()).map_err(|e| e.to_string())?;
    if parsed.session_type.trim().is_empty() {
        return Err("session_type is empty".to_string());
    }
    if parsed.session_type.trim() != parsed.session_type {
        return Err(format!(
            "session_type {:?} has surrounding whitespace",
            parsed.session_type
        ));
    }
    if is_no_agent_session_type(&parsed.session_type) {
        return Err(format!(
            "session_type {NO_AGENT_SESSION_TYPE:?} is reserved for a session with no agent; a \
             tool file cannot declare it"
        ));
    }
    if parsed.label.trim().is_empty() {
        return Err("label is empty".to_string());
    }
    Ok(parsed)
}

/// The [`ExtensionOwner`] of [`SESSION_EXTENSION`].
#[derive(Debug, Clone, Copy, Default)]
pub struct SessionExtension;

impl ExtensionOwner for SessionExtension {
    fn name(&self) -> &str {
        SESSION_EXTENSION
    }

    fn validate(&self, _tool_id: &str, section: &serde_yaml::Value) -> Result<(), String> {
        parse_session_section(section).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn parse(yaml: &str) -> Result<SessionSection, String> {
        parse_session_section(&serde_yaml::from_str(yaml).unwrap())
    }

    #[test]
    fn a_section_reads_its_session_type_launch_and_label() {
        // When
        let parsed = parse("{ session_type: tool, launch: tddy_coder, label: Tool }");

        // Then
        assert_eq!(
            parsed,
            Ok(SessionSection {
                session_type: "tool".to_string(),
                launch: LaunchKind::TddyCoder,
                label: "Tool".to_string(),
            })
        );
    }

    #[test]
    fn the_no_agent_word_is_refused() {
        // When
        let refused = parse("{ session_type: workspace, launch: agent_cli, label: W }");

        // Then
        assert_eq!(
            refused,
            Err(
                "session_type \"workspace\" is reserved for a session with no agent; a tool file \
                 cannot declare it"
                    .to_string()
            )
        );
    }

    #[test]
    fn an_unknown_launch_kind_is_refused() {
        // When
        let refused = parse("{ session_type: x-cli, launch: shell, label: X }");

        // Then
        assert!(refused.is_err(), "shell is no launch kind: {refused:?}");
    }

    #[test]
    fn an_empty_session_type_is_refused() {
        // When
        let refused = parse("{ session_type: '', launch: agent_cli, label: X }");

        // Then
        assert_eq!(refused, Err("session_type is empty".to_string()));
    }
}
