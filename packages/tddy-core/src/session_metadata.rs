//! Session metadata for daemon session discovery.
//!
//! Stored as `.session.yaml` in each session directory.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Session metadata written to .session.yaml for daemon session listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionMetadata {
    pub session_id: String,
    pub project_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub status: String,
    #[serde(default)]
    pub repo_path: Option<String>,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub livekit_room: Option<String>,
    /// When true, the workflow is waiting on the user (plan/doc approval, clarifications, etc.).
    #[serde(default)]
    pub pending_elicitation: bool,
    /// Optional parent session when this session was created as a chain child (PRD: session chaining).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_session_id: Option<String>,
    /// Session type: "tool" (default/empty) or "claude-cli". Absent in legacy files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_type: Option<String>,
    /// Model id for claude-cli sessions (e.g. "opus", "claude-opus-5"). Absent in legacy files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Cursor chat id (`cursor-agent create-chat`) this cursor-cli session talks to. Every spawn
    /// for the session passes `--resume <id>`, so a resume continues the same chat instead of
    /// opening a new one. Absent for non-cursor sessions and legacy files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor_chat_id: Option<String>,
    /// Granular activity status reported by per-worktree claude-cli hooks (e.g. "Running",
    /// "WaitingForInput"). Absent for tool sessions and legacy files. Set by `ReportSessionStatus`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity_status: Option<String>,
    /// Per-session token authorising claude-cli hooks to call `ReportSessionStatus`. Generated at
    /// session-start, persisted here, and baked into the worktree hook command. Absent for tool
    /// sessions and legacy files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_token: Option<String>,
    /// When true, the claude-cli session runs inside a platform sandbox (darwin Seatbelt).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<bool>,
    /// Coding agent the session was started with (e.g. "cursor", "claude"). Persisted so a resume
    /// restores the same agent instead of falling back to the default. Absent in legacy files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Workflow recipe the session was started with (e.g. "pr-stack"). Persisted so a resume
    /// restores the same recipe. Absent in legacy files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe: Option<String>,
    /// Specialized-agent names wired into a sandboxed claude-cli session (array model). Empty for
    /// non-subagent sessions, legacy-single-subagent sessions, and legacy files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub specialized_agents: Vec<String>,
    /// Daemon instance holding this session's worktree, when the agent runs on another daemon
    /// (docs/ft/daemon/remote-managed-worktree.md). Absent for co-located sessions and legacy
    /// files. Persisted, unlike `SessionEntry.daemon_instance_id`, which is stamped at read time —
    /// a split session cannot be attributed by "who answered ListSessions", because two daemons
    /// each legitimately hold one half.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codebase_daemon_instance_id: Option<String>,
    /// The paired `workspace` session on that daemon whose worktree this session works in. Absent
    /// for co-located sessions and legacy files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codebase_session_id: Option<String>,
}

pub const SESSION_METADATA_FILENAME: &str = ".session.yaml";

/// Options for [`write_initial_tool_session_metadata`] (CLI, gRPC daemon, LiveKit, TUI).
#[derive(Debug, Clone, Default)]
pub struct InitialToolSessionMetadataOpts {
    pub project_id: String,
    pub repo_path: Option<String>,
    pub pid: Option<u32>,
    pub tool: Option<String>,
    pub livekit_room: Option<String>,
    /// When set, `.session.yaml` records the stacked-from parent session id.
    pub previous_session_id: Option<String>,
    /// Session type: "tool" (default/empty) or "claude-cli".
    pub session_type: Option<String>,
    /// Model id for claude-cli sessions.
    pub model: Option<String>,
    /// Initial granular activity status for claude-cli sessions. Usually `None` for tool sessions.
    pub activity_status: Option<String>,
    /// Per-session hook token for claude-cli sessions. `None` for tool sessions.
    pub hook_token: Option<String>,
    /// When true, the claude-cli session runs inside a platform sandbox (darwin Seatbelt).
    pub sandbox: Option<bool>,
    /// Coding agent the session was started with (e.g. "cursor", "claude").
    pub agent: Option<String>,
    /// Workflow recipe the session was started with (e.g. "pr-stack").
    pub recipe: Option<String>,
}

/// Writes `.session.yaml` for a newly created session directory.
///
/// `session_id` is taken from `session_dir`'s final path segment so it stays aligned with the
/// on-disk layout (`…/sessions/<id>/`).
pub fn write_initial_tool_session_metadata(
    session_dir: &Path,
    opts: InitialToolSessionMetadataOpts,
) -> Result<(), crate::WorkflowError> {
    let session_id = session_dir
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            crate::WorkflowError::WriteFailed(
                "write_initial_tool_session_metadata: session_dir has no usable basename"
                    .to_string(),
            )
        })?
        .to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let metadata = SessionMetadata {
        session_id,
        project_id: opts.project_id,
        created_at: now.clone(),
        updated_at: now,
        status: "active".to_string(),
        repo_path: opts.repo_path,
        pid: opts.pid,
        tool: opts.tool,
        livekit_room: opts.livekit_room,
        pending_elicitation: false,
        previous_session_id: opts.previous_session_id,
        session_type: opts.session_type,
        model: opts.model,
        cursor_chat_id: None,
        activity_status: opts.activity_status,
        hook_token: opts.hook_token,
        sandbox: opts.sandbox,
        agent: opts.agent,
        recipe: opts.recipe,
        specialized_agents: Vec::new(),
        // Split placement is claude-cli only, and those sessions write their metadata directly —
        // a session created here always works in a worktree on this host.
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
    };
    write_session_metadata(session_dir, &metadata)
}

/// Write session metadata to the session directory.
///
/// Goes through [`crate::atomic_file::write_atomic`]: a session whose `.session.yaml` is
/// truncated is a session the daemon and the web can no longer see, even while its agent process
/// is alive, so a failed rewrite must leave the previous file standing rather than empty it.
pub fn write_session_metadata(
    session_dir: &Path,
    metadata: &SessionMetadata,
) -> Result<(), crate::WorkflowError> {
    let path = session_dir.join(SESSION_METADATA_FILENAME);
    let contents = serde_yaml::to_string(metadata)
        .map_err(|e| crate::WorkflowError::WriteFailed(e.to_string()))?;
    crate::atomic_file::write_atomic_labelled(&path, contents)
        .map_err(crate::WorkflowError::WriteFailed)
}

/// Atomically update the `activity_status` field in an existing `.session.yaml`.
///
/// Reads the metadata, sets `activity_status = Some(status.to_string())`, bumps `updated_at`,
/// and writes it back. All other fields are preserved.
///
/// Used by the `ReportSessionStatus` gRPC handler to record the latest hook-reported status.
pub fn update_activity_status(
    session_dir: &Path,
    status: &str,
) -> Result<(), crate::WorkflowError> {
    let mut metadata = read_session_metadata(session_dir)?;
    metadata.activity_status = Some(status.to_string());
    metadata.updated_at = chrono::Utc::now().to_rfc3339();
    write_session_metadata(session_dir, &metadata)
}

/// Read session metadata from the session directory.
pub fn read_session_metadata(session_dir: &Path) -> Result<SessionMetadata, crate::WorkflowError> {
    let path = session_dir.join(SESSION_METADATA_FILENAME);
    let contents = std::fs::read_to_string(&path)
        .map_err(|e| crate::WorkflowError::WriteFailed(e.to_string()))?;
    serde_yaml::from_str(&contents).map_err(|e| crate::WorkflowError::WriteFailed(e.to_string()))
}

/// The checkout a session directory names as its repo root, or `None` when nothing names one.
///
/// Two files can record it, and they are written at different moments:
///
/// - `changeset.yaml` gains `repo_path` only when the session is given a worktree of its own,
/// - `.session.yaml` records the checkout the session was started over, for **every** session.
///
/// A pr-stack orchestrator is a planning session that never creates a worktree, so its changeset
/// names no repo at all and the metadata is the only record. A missing or unreadable
/// `changeset.yaml` is likewise not an answer about the repo, so it falls through the same way.
///
/// `None` means *the repo is unknown*, and callers must report it as unknown: substituting the
/// session directory points git at a path that is not a repository, which is how a merged PR came to
/// read as "no PR exists" (PRD: docs/ft/coder/pr-stack-live-status.md, C3/D8).
#[must_use]
pub fn repo_root_for_session(session_dir: &Path) -> Option<PathBuf> {
    crate::read_changeset(session_dir)
        .ok()
        .and_then(|changeset| changeset.repo_path)
        .or_else(|| read_session_metadata(session_dir).ok()?.repo_path)
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn write_initial_tool_session_metadata_uses_dir_basename_as_session_id() {
        let tmp =
            std::env::temp_dir().join(format!("tddy-session-meta-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let sid = "018f1234-5678-7abc-8def-123456789abc";
        let session_dir = tmp.join("sessions").join(sid);
        fs::create_dir_all(&session_dir).unwrap();

        write_initial_tool_session_metadata(
            &session_dir,
            InitialToolSessionMetadataOpts {
                project_id: "proj-1".to_string(),
                repo_path: Some("/repo".to_string()),
                pid: Some(4242),
                tool: Some("tddy-coder".to_string()),
                livekit_room: None,
                previous_session_id: None,
                session_type: None,
                model: None,

                activity_status: None,
                hook_token: None,
                sandbox: None,
                agent: None,
                recipe: None,
            },
        )
        .unwrap();

        let read = read_session_metadata(&session_dir).unwrap();
        assert_eq!(read.session_id, sid);
        assert_eq!(read.project_id, "proj-1");
        assert_eq!(read.status, "active");
        assert_eq!(read.repo_path.as_deref(), Some("/repo"));
        assert_eq!(read.pid, Some(4242));
        assert_eq!(read.tool.as_deref(), Some("tddy-coder"));
        assert!(read.livekit_room.is_none());
        assert!(!read.pending_elicitation);
        assert!(read.previous_session_id.is_none());

        let _ = fs::remove_dir_all(&tmp);
    }

    /// **claude_cli_metadata_round_trip** — `.session.yaml` must preserve `session_type` and
    /// `model` through a write/read cycle.
    #[test]
    fn claude_cli_metadata_round_trip() {
        let tmp = std::env::temp_dir().join(format!(
            "tddy-session-meta-claude-cli-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        let sid = "01900000-0000-7000-8000-000000000cli";
        let session_dir = tmp.join("sessions").join(sid);
        fs::create_dir_all(&session_dir).unwrap();

        write_initial_tool_session_metadata(
            &session_dir,
            InitialToolSessionMetadataOpts {
                project_id: "proj-claude".to_string(),
                repo_path: Some("/tmp/worktrees/claude-cli-01900000".to_string()),
                pid: Some(7777),
                tool: None,
                livekit_room: None,
                previous_session_id: None,
                session_type: Some("claude-cli".to_string()), // NEW FIELD — compile error
                model: Some("claude-sonnet-4-6".to_string()), // NEW FIELD — compile error

                activity_status: None,
                hook_token: None,
                sandbox: None,
                agent: None,
                recipe: None,
            },
        )
        .unwrap();

        let read = read_session_metadata(&session_dir).unwrap();
        assert_eq!(
            read.session_type.as_deref(),
            Some("claude-cli"),
            "session_type must survive write/read round-trip"
        );
        assert_eq!(
            read.model.as_deref(),
            Some("claude-sonnet-4-6"),
            "model must survive write/read round-trip"
        );

        // Verify that legacy .session.yaml without session_type/model still deserializes (backward
        // compatibility: both fields must have #[serde(default)]).
        let legacy_yaml = format!(
            r#"session_id: {sid}
project_id: proj-legacy
created_at: "2026-01-01T00:00:00Z"
updated_at: "2026-01-01T00:00:00Z"
status: active
"#
        );
        let legacy: SessionMetadata = serde_yaml::from_str(&legacy_yaml)
            .expect("legacy .session.yaml without session_type/model must deserialize");
        assert!(
            legacy.session_type.is_none(),
            "session_type must default to None for legacy sessions"
        );
        assert!(
            legacy.model.is_none(),
            "model must default to None for legacy sessions"
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    /// A tool session must persist which coding agent and workflow recipe it was started with,
    /// so a later resume can restore them instead of falling back to the default agent (`claude`).
    /// This is the root cause behind a resumed `cursor` / `pr-stack` session silently running as
    /// `claude`: `.session.yaml` never carried the agent/recipe, so resume had nothing to restore.
    #[test]
    fn agent_and_recipe_round_trip_through_session_yaml() {
        // Given a fresh tool session started with the cursor agent on the pr-stack recipe
        let tmp = std::env::temp_dir().join(format!("tddy-agent-recipe-rt-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let session_dir = tmp
            .join("sessions")
            .join("019f243a-8e31-7203-81dd-53f5ef8b9352");
        fs::create_dir_all(&session_dir).unwrap();

        write_initial_tool_session_metadata(
            &session_dir,
            InitialToolSessionMetadataOpts {
                project_id: "proj-prstack".to_string(),
                agent: Some("cursor".to_string()),
                recipe: Some("pr-stack".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        // When the metadata is read back (as resume does)
        let read = read_session_metadata(&session_dir).unwrap();

        // Then both the agent and the recipe survive the round-trip
        assert_eq!(
            read.agent.as_deref(),
            Some("cursor"),
            "agent must survive write/read round-trip so resume restores it"
        );
        assert_eq!(
            read.recipe.as_deref(),
            Some("pr-stack"),
            "recipe must survive write/read round-trip so resume restores it"
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    /// A legacy `.session.yaml` written before agent/recipe were persisted must still deserialize,
    /// with both fields defaulting to `None`.
    #[test]
    fn legacy_session_yaml_without_agent_recipe_defaults_to_none() {
        // Given a legacy metadata file with no agent/recipe keys
        let yaml = r#"session_id: legacy-sess
project_id: proj-legacy
created_at: "2026-01-01T00:00:00Z"
updated_at: "2026-01-01T00:00:00Z"
status: active
"#;

        // When it is deserialized
        let meta: SessionMetadata =
            serde_yaml::from_str(yaml).expect("legacy YAML must deserialise");

        // Then agent and recipe default to None
        assert!(
            meta.agent.is_none(),
            "agent must default to None for legacy sessions"
        );
        assert!(
            meta.recipe.is_none(),
            "recipe must default to None for legacy sessions"
        );
    }

    /// `activity_status` survives a write/read round-trip through `.session.yaml`.
    #[test]
    fn activity_status_round_trips_through_session_yaml() {
        let tmp =
            std::env::temp_dir().join(format!("tddy-activity-status-rt-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let session_dir = tmp.join("sessions").join("sess-act-rt");
        fs::create_dir_all(&session_dir).unwrap();

        write_initial_tool_session_metadata(
            &session_dir,
            InitialToolSessionMetadataOpts {
                project_id: "proj-rt".to_string(),
                session_type: Some("claude-cli".to_string()),
                model: Some("claude-sonnet-4-6".to_string()),
                activity_status: Some("Running".to_string()),
                hook_token: None,
                ..Default::default()
            },
        )
        .unwrap();

        let read = read_session_metadata(&session_dir).unwrap();
        assert_eq!(
            read.activity_status.as_deref(),
            Some("Running"),
            "activity_status must survive write/read round-trip"
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    /// `hook_token` is omitted from the YAML when `None` (no key present in file).
    #[test]
    fn hook_token_omitted_when_none() {
        let tmp = std::env::temp_dir().join(format!("tddy-hook-token-none-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let session_dir = tmp.join("sessions").join("sess-ht-none");
        fs::create_dir_all(&session_dir).unwrap();

        write_initial_tool_session_metadata(
            &session_dir,
            InitialToolSessionMetadataOpts {
                project_id: "proj-ht".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let yaml_text =
            std::fs::read_to_string(session_dir.join(SESSION_METADATA_FILENAME)).unwrap();
        assert!(
            !yaml_text.contains("hook_token"),
            "hook_token must not appear in YAML when None; got:\n{yaml_text}"
        );
        assert!(
            !yaml_text.contains("activity_status"),
            "activity_status must not appear in YAML when None; got:\n{yaml_text}"
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    /// Legacy `.session.yaml` without `activity_status` or `hook_token` must still deserialise
    /// (both fields have `#[serde(default)]`).
    #[test]
    fn legacy_session_yaml_without_new_fields_deserializes() {
        let yaml = r#"session_id: old-sess
project_id: proj-legacy
created_at: "2026-01-01T00:00:00Z"
updated_at: "2026-01-01T00:00:00Z"
status: active
"#;
        let meta: SessionMetadata =
            serde_yaml::from_str(yaml).expect("legacy YAML must deserialise");
        assert!(
            meta.activity_status.is_none(),
            "activity_status must default to None"
        );
        assert!(meta.hook_token.is_none(), "hook_token must default to None");
    }

    /// **The disk-full regression.** A rewrite that cannot complete must leave the session
    /// listable: the old `.session.yaml` stays whole rather than becoming the 0-byte file that
    /// makes a running session disappear from the daemon and the web.
    ///
    /// A read-only session directory stands in for the full filesystem — both make the write
    /// impossible, and the point of the fix is that the impossibility lands on a swap file.
    #[cfg(unix)]
    #[test]
    fn failed_metadata_rewrite_leaves_the_session_readable() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = std::env::temp_dir().join(format!("tddy-nospace-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let session_dir = tmp.join("sessions").join("sess-nospace");
        fs::create_dir_all(&session_dir).unwrap();
        write_initial_tool_session_metadata(
            &session_dir,
            InitialToolSessionMetadataOpts {
                project_id: "proj-nospace".to_string(),
                activity_status: Some("Running".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        fs::set_permissions(&session_dir, fs::Permissions::from_mode(0o555)).unwrap();
        // root ignores permission bits, leaving nothing for this case to observe.
        let unwritable = std::fs::File::create(session_dir.join(".probe")).is_err();

        let result = update_activity_status(&session_dir, "WaitingForInput");
        let after = read_session_metadata(&session_dir);
        fs::set_permissions(&session_dir, fs::Permissions::from_mode(0o755)).unwrap();
        let _ = fs::remove_dir_all(&tmp);

        if !unwritable {
            return;
        }
        assert!(
            result.is_err(),
            "a write that cannot complete must report it"
        );
        let after = after.expect("`.session.yaml` must still parse after a failed rewrite");
        assert_eq!(
            after.activity_status.as_deref(),
            Some("Running"),
            "the previous metadata must survive intact, not be half-replaced"
        );
    }

    /// `update_activity_status` must overwrite only `activity_status` and bump `updated_at`;
    /// all other fields (including `status`) must be unchanged.
    #[test]
    fn update_activity_status_overwrites_only_status_and_bumps_updated_at() {
        let tmp = std::env::temp_dir().join(format!("tddy-upd-act-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let session_dir = tmp.join("sessions").join("sess-upd");
        fs::create_dir_all(&session_dir).unwrap();

        let original_updated_at = "2026-06-13T10:00:00Z";
        write_initial_tool_session_metadata(
            &session_dir,
            InitialToolSessionMetadataOpts {
                project_id: "proj-upd".to_string(),
                session_type: Some("claude-cli".to_string()),
                activity_status: Some("Started".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        // Manually set a known `updated_at` for comparison.
        {
            let mut meta = read_session_metadata(&session_dir).unwrap();
            meta.updated_at = original_updated_at.to_string();
            write_session_metadata(&session_dir, &meta).unwrap();
        }

        update_activity_status(&session_dir, "WaitingForInput")
            .expect("update_activity_status must succeed");

        let updated = read_session_metadata(&session_dir).unwrap();
        assert_eq!(
            updated.activity_status.as_deref(),
            Some("WaitingForInput"),
            "activity_status must be updated to WaitingForInput"
        );
        assert_eq!(
            updated.status, "active",
            "session status field must remain 'active'"
        );
        assert_ne!(
            updated.updated_at, original_updated_at,
            "updated_at must be bumped by update_activity_status"
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    /// **chain_child_metadata_records_previous_session_id** — `.session.yaml` must allow optional
    /// `previous_session_id` on [`SessionMetadata`] (PRD: chain child observability).
    #[test]
    fn chain_child_metadata_records_previous_session_id() {
        let sid = "018f1234-5678-7abc-8def-123456789abc";
        let prev = "019f1234-5678-7abc-8def-123456789abc";
        let yaml = format!(
            r#"session_id: {sid}
project_id: proj-chain
created_at: "2026-05-01T12:00:00Z"
updated_at: "2026-05-01T12:00:00Z"
status: active
previous_session_id: {prev}
"#
        );
        assert!(
            serde_yaml::from_str::<SessionMetadata>(&yaml).is_ok(),
            "SessionMetadata must accept optional previous_session_id and deserialize from .session.yaml (PRD session chaining)"
        );
    }

    /// `specialized_agents` (array-of-agent-names model) survives a write/read round-trip through
    /// `.session.yaml` — needed so a daemon-hosted sandboxed claude-cli session can reconstruct its
    /// specialized-agent config on resume.
    #[test]
    fn specialized_agents_round_trips_through_session_yaml() {
        let tmp = std::env::temp_dir().join(format!(
            "tddy-session-meta-specialized-rt-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        let session_dir = tmp.join("sessions").join("sess-specialized-rt");
        fs::create_dir_all(&session_dir).unwrap();

        let metadata = SessionMetadata {
            session_id: "sess-specialized-rt".to_string(),
            project_id: "proj-specialized".to_string(),
            created_at: "2026-07-02T10:00:00Z".to_string(),
            updated_at: "2026-07-02T10:00:00Z".to_string(),
            status: "active".to_string(),
            repo_path: Some("/tmp/worktrees/specialized".to_string()),
            pid: Some(1234),
            tool: None,
            livekit_room: None,
            pending_elicitation: false,
            previous_session_id: None,
            session_type: Some("claude-cli".to_string()),
            model: Some("claude-sonnet-4-6".to_string()),
            cursor_chat_id: None,
            activity_status: None,
            hook_token: None,
            sandbox: Some(true),
            agent: None,
            recipe: None,
            specialized_agents: vec!["fastcontext".to_string(), "my-linter".to_string()],
            codebase_daemon_instance_id: None,
            codebase_session_id: None,
        };
        write_session_metadata(&session_dir, &metadata).unwrap();

        let read = read_session_metadata(&session_dir).unwrap();
        assert_eq!(
            read.specialized_agents,
            vec!["fastcontext".to_string(), "my-linter".to_string()]
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    /// `specialized_agents` is omitted from the YAML when empty (no key present in file).
    #[test]
    fn specialized_agents_omitted_from_yaml_when_empty() {
        let tmp = std::env::temp_dir().join(format!(
            "tddy-session-meta-specialized-empty-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        let session_dir = tmp.join("sessions").join("sess-specialized-empty");
        fs::create_dir_all(&session_dir).unwrap();

        write_initial_tool_session_metadata(
            &session_dir,
            InitialToolSessionMetadataOpts {
                project_id: "proj-specialized-empty".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let yaml_text =
            std::fs::read_to_string(session_dir.join(SESSION_METADATA_FILENAME)).unwrap();
        assert!(
            !yaml_text.contains("specialized_agents"),
            "specialized_agents must not appear in YAML when empty; got:\n{yaml_text}"
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    /// Legacy `.session.yaml` without `specialized_agents` must still deserialize, defaulting to
    /// an empty vec (`#[serde(default)]`).
    #[test]
    fn legacy_session_yaml_without_specialized_agents_defaults_to_empty() {
        let yaml = r#"session_id: old-sess
project_id: proj-legacy
created_at: "2026-01-01T00:00:00Z"
updated_at: "2026-01-01T00:00:00Z"
status: active
"#;
        let meta: SessionMetadata =
            serde_yaml::from_str(yaml).expect("legacy YAML must deserialise");
        assert!(
            meta.specialized_agents.is_empty(),
            "specialized_agents must default to empty vec for legacy sessions"
        );
    }
}
