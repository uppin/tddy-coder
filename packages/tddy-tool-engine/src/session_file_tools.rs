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
use serde_json::{json, Value};
use tddy_worktree_service::session_target::HomeCaller;

use crate::catalog::ToolDef;
use crate::project_home_tools::tool_def;
use crate::ToolOutcome;

pub const WRITE_SESSION_FILE: &str = "WriteSessionFile";
pub const LIST_SESSION_FILES: &str = "ListSessionFiles";

/// Whether `name` is one of the session-file tools.
pub fn is_session_file_tool(name: &str) -> bool {
    matches!(name, WRITE_SESSION_FILE | LIST_SESSION_FILES)
}

/// The two tools' definitions.
pub fn session_file_tool_defs() -> Vec<ToolDef> {
    vec![
        tool_def(
            WRITE_SESSION_FILE,
            "Write a file into your own session's files: a brief, a plan, a spec. Writing a name again replaces it. Answers the file's name and size, and 'attachment': the entry to put in StartSession's attachments[] to hand the file to the session you start (it gets a copy). Names are one flat file name (no '/', no '..', no leading '.'), at most 128 bytes; content at most 1 MiB.",
            json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "minLength": 1,
                        "description": "One flat file name, e.g. brief.md."
                    },
                    "content": {
                        "type": "string",
                        "description": "The file's text."
                    }
                },
                "required": ["name", "content"],
                "additionalProperties": false
            }),
        ),
        tool_def(
            LIST_SESSION_FILES,
            "List the files you wrote into your own session's files, by name, with their sizes and each one's StartSession attachment.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        ),
    ]
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
    async fn write_session_file(
        &self,
        caller: &HomeCaller,
        args: WriteSessionFileArgs,
    ) -> Result<SessionFileEntry, String>;

    /// `caller`'s session files, by name.
    async fn list_session_files(
        &self,
        caller: &HomeCaller,
    ) -> Result<Vec<SessionFileEntry>, String>;
}

/// The `StartSession` `attachments[]` entry that attaches `caller`'s session file `name`.
pub fn attachment_for(caller: &HomeCaller, name: &str) -> Value {
    json!({
        "basename": name,
        "scope": "HOST_DOCUMENT_SCOPE_SESSION_ARTIFACT",
        "session_id": caller.session_id,
        "relative_path": format!("files/{name}"),
    })
}

/// Run a session-file tool for `caller`. A refusal comes back as an `is_error` outcome naming it;
/// the role refusal is the daemon's and happens before this is called.
pub async fn execute_session_file_tool(
    port: &dyn SessionFilePort,
    caller: &HomeCaller,
    tool_name: &str,
    args_json: &str,
) -> ToolOutcome {
    let args: Value = match serde_json::from_str(args_json) {
        Ok(args) => args,
        Err(e) => return ToolOutcome::err(format!("{tool_name}: invalid args_json: {e}")),
    };
    match tool_name {
        WRITE_SESSION_FILE => {
            let args: WriteSessionFileArgs = match serde_json::from_value(args) {
                Ok(args) => args,
                Err(e) => {
                    return ToolOutcome::err(format!(
                        "{tool_name}: name and content are required: {e}"
                    ))
                }
            };
            match port.write_session_file(caller, args).await {
                Ok(entry) => {
                    let mut answer = entry_json(&entry);
                    answer["attachment"] = attachment_for(caller, &entry.name);
                    ToolOutcome::ok(answer.to_string())
                }
                Err(message) => ToolOutcome::err(format!("{tool_name}: {message}")),
            }
        }
        LIST_SESSION_FILES => match port.list_session_files(caller).await {
            Ok(entries) => {
                let files: Vec<Value> = entries
                    .iter()
                    .map(|entry| {
                        let mut file = entry_json(entry);
                        file["attachment"] = attachment_for(caller, &entry.name);
                        file
                    })
                    .collect();
                ToolOutcome::ok(json!({ "files": files }).to_string())
            }
            Err(message) => ToolOutcome::err(format!("{tool_name}: {message}")),
        },
        other => ToolOutcome::err(format!("unknown session file tool: {other}")),
    }
}

fn entry_json(entry: &SessionFileEntry) -> Value {
    json!({
        "name": entry.name,
        "size_bytes": entry.size_bytes,
        "modified_unix_ms": entry.modified_unix_ms,
    })
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
