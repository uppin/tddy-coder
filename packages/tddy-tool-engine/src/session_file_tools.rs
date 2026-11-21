//! `WriteSessionFile` and `ListSessionFiles`: an orchestrator writes briefs, plans and specs into its
//! own session directory and attaches them to the sessions it starts (`#orchestra` 3/9).
//!
//! A session file lives in the orchestrator's `SESSION_ARTIFACT` scope, under `files/<name>`, so
//! `WriteSessionFile` answers with the exact `attachments[]` entry `StartSession` takes for it: the
//! child gets a copy at start, as with any attached host document, and never reads the
//! orchestrator's directory itself.
//!
//! Served to orchestrator roles; the write goes into the caller's own session directory and nowhere
//! else, so it needs no grant. Feature doc: docs/ft/coder/session-attachments.md.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use tddy_worktree_service::session_target::HomeCaller;

use crate::catalog::ToolDef;
use crate::ToolOutcome;

pub const WRITE_SESSION_FILE: &str = "WriteSessionFile";
pub const LIST_SESSION_FILES: &str = "ListSessionFiles";

/// Whether `name` is one of the session-file tools.
pub fn is_session_file_tool(name: &str) -> bool {
    matches!(name, WRITE_SESSION_FILE | LIST_SESSION_FILES)
}

/// The two tools' definitions.
pub fn session_file_tool_defs() -> Vec<ToolDef> {
    // TODO(session-files): implement
    unimplemented!("session_file_tool_defs")
}

/// [`WRITE_SESSION_FILE`]'s arguments.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriteSessionFileArgs {
    /// One flat file name: no `/`, no `..`, no leading `.`.
    pub name: String,
    pub content: String,
}

/// One session file as the tools report it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionFileEntry {
    pub name: String,
    pub size_bytes: u64,
    pub modified_unix_ms: u64,
}

/// What the tools need from the daemon: the caller's own session files.
#[async_trait]
pub trait SessionFilePort: Send + Sync {
    /// Write `args` into `caller`'s session files. An error names why it was not written.
    ///
    /// The default refuses, so a port that keeps no session files need not name it.
    async fn write_session_file(
        &self,
        caller: &HomeCaller,
        args: WriteSessionFileArgs,
    ) -> Result<SessionFileEntry, String> {
        let _ = (caller, args);
        Err("this host keeps no session files".to_string())
    }

    /// `caller`'s session files, by name.
    async fn list_session_files(
        &self,
        caller: &HomeCaller,
    ) -> Result<Vec<SessionFileEntry>, String> {
        let _ = caller;
        Err("this host keeps no session files".to_string())
    }
}

/// The `StartSession` `attachments[]` entry that attaches `caller`'s session file `name`.
pub fn attachment_for(caller: &HomeCaller, name: &str) -> Value {
    // TODO(session-files): implement
    let _ = (caller, name);
    unimplemented!("attachment_for")
}

/// Run a session-file tool for `caller`. A refusal comes back as an `is_error` outcome naming it;
/// the role refusal is the daemon's and happens before this is called.
pub async fn execute_session_file_tool(
    port: &dyn SessionFilePort,
    caller: &HomeCaller,
    tool_name: &str,
    args_json: &str,
) -> ToolOutcome {
    // TODO(session-files): implement
    let _ = (port, caller, tool_name, args_json);
    unimplemented!("execute_session_file_tool")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn the_orchestrator() -> HomeCaller {
        HomeCaller {
            session_id: "01a1fdd8-dea7-7e70-9945-d66127579ec2".to_string(),
            project_id: "019d105b-ac0f-78d3-9a89-409731145a3a".to_string(),
        }
    }

    #[test]
    fn a_session_files_attachment_is_its_session_artifact_under_files() {
        let attachment = attachment_for(&the_orchestrator(), "brief.md");

        assert_eq!(
            attachment,
            json!({
                "basename": "brief.md",
                "scope": "HOST_DOCUMENT_SCOPE_SESSION_ARTIFACT",
                "session_id": "01a1fdd8-dea7-7e70-9945-d66127579ec2",
                "relative_path": "files/brief.md"
            })
        );
    }

    #[test]
    fn the_attachment_parses_as_a_start_session_attachment() {
        let attachment = attachment_for(&the_orchestrator(), "brief.md");

        let parsed: Result<crate::start_session_args::HostDocumentAttachment, _> =
            serde_json::from_value(attachment);

        assert!(parsed.is_ok(), "{parsed:?}");
    }

    #[test]
    fn both_tools_are_defined_with_their_names() {
        let names: Vec<String> = session_file_tool_defs()
            .into_iter()
            .map(|d| d.name)
            .collect();

        assert_eq!(
            names,
            vec![
                WRITE_SESSION_FILE.to_string(),
                LIST_SESSION_FILES.to_string()
            ]
        );
    }
}
