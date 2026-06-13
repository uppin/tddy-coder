//! Session directory → Connection list display fields (TUI status bar parity).
//!
//! Used when building enriched `SessionEntry` for `ListSessions`. Mapping from
//! `.session.yaml` and `changeset.yaml` (parity with TUI `format_status_bar`):
//!
//! | Column (web)     | Source |
//! |------------------|--------|
//! | Goal             | `changeset.yaml` → session row matching `.session.yaml` `session_id`: `tag` (workflow goal id) |
//! | Workflow (state) | `changeset.state.current` (named workflow state, e.g. Red, Init) |
//! | Elapsed          | Wall time since the last `state.history` transition whose `state` matches `state.current` (`at` RFC3339); if none, `state.updated_at` |
//! | Agent            | Matching session row: `agent` |
//! | Model            | `changeset.models[tag]` for that row’s `tag` |

use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tddy_core::{
    format_elapsed_compact, read_changeset, read_session_metadata, Changeset,
    SessionEntry as CsSessionEntry,
};
use tddy_service::proto::connection::SessionEntry as ProtoSessionEntry;

/// Display strings aligned with the TUI status bar (goal, state, elapsed, agent, model).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionListStatusDisplay {
    pub workflow_goal: String,
    pub workflow_state: String,
    pub elapsed_display: String,
    pub agent: String,
    pub model: String,
    /// Granular activity status for claude-cli sessions (e.g. "Running", "WaitingForInput").
    /// Empty string for tool/workflow sessions.
    pub activity_status: String,
}

impl SessionListStatusDisplay {
    fn all_placeholders() -> Self {
        let p = "—".to_string();
        Self {
            workflow_goal: p.clone(),
            workflow_state: p.clone(),
            elapsed_display: p.clone(),
            agent: p.clone(),
            model: p,
            activity_status: String::new(),
        }
    }
}

fn parse_rfc3339_utc(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

/// Instant when the current workflow state began (last history entry matching `state.current`).
fn current_step_started_at(changeset: &Changeset) -> Option<DateTime<Utc>> {
    let current = &changeset.state.current;
    for t in changeset.state.history.iter().rev() {
        if t.state == *current {
            return parse_rfc3339_utc(&t.at);
        }
    }
    parse_rfc3339_utc(&changeset.state.updated_at)
}

/// Human-readable elapsed for the current workflow step from persisted `changeset.yaml` timestamps.
pub fn elapsed_display_for_changeset(changeset: &Changeset) -> String {
    let Some(start) = current_step_started_at(changeset) else {
        log::debug!(
            target: "tddy_daemon::session_list_enrichment",
            "elapsed_display: could not parse step start time"
        );
        return "—".to_string();
    };
    let now = Utc::now();
    let delta = now.signed_duration_since(start);
    let secs = delta.num_seconds();
    if secs < 0 {
        log::debug!(
            target: "tddy_daemon::session_list_enrichment",
            "elapsed_display: negative delta (clock skew?), using 0s"
        );
        return format_elapsed_compact(Duration::ZERO);
    }
    let d = Duration::from_secs(secs as u64);
    let out = format_elapsed_compact(d);
    log::debug!(
        target: "tddy_daemon::session_list_enrichment",
        "elapsed_display: start={start} now={now} -> {out}"
    );
    out
}

fn find_session_row<'a>(changeset: &'a Changeset, session_id: &str) -> Option<&'a CsSessionEntry> {
    changeset.sessions.iter().find(|s| s.id == session_id)
}

/// Derive list-row status fields from a session directory (`.session.yaml`, `changeset.yaml`).
pub fn session_list_status_from_session_dir(
    session_dir: &Path,
) -> anyhow::Result<SessionListStatusDisplay> {
    log::info!(
        target: "tddy_daemon::session_list_enrichment",
        "enriching session list row for {}",
        session_dir.display()
    );

    let meta = match read_session_metadata(session_dir) {
        Ok(m) => m,
        Err(e) => {
            log::debug!(
                target: "tddy_daemon::session_list_enrichment",
                "no readable .session.yaml in {}: {e}",
                session_dir.display()
            );
            return Ok(SessionListStatusDisplay::all_placeholders());
        }
    };

    // Claude CLI sessions have no changeset — read agent/model/activity_status from metadata.
    if meta.session_type.as_deref() == Some("claude-cli") {
        return Ok(SessionListStatusDisplay {
            workflow_goal: String::new(),
            workflow_state: String::new(),
            elapsed_display: String::new(),
            agent: "claude-cli".to_string(),
            model: meta.model.clone().unwrap_or_default(),
            activity_status: meta.activity_status.clone().unwrap_or_default(),
        });
    }

    let changeset = match read_changeset(session_dir) {
        Ok(c) => c,
        Err(e) => {
            log::debug!(
                target: "tddy_daemon::session_list_enrichment",
                "no readable changeset.yaml in {}: {e}",
                session_dir.display()
            );
            return Ok(SessionListStatusDisplay::all_placeholders());
        }
    };

    let workflow_state = changeset.state.current.to_string();
    let elapsed_display = elapsed_display_for_changeset(&changeset);

    let Some(row) = find_session_row(&changeset, &meta.session_id) else {
        log::debug!(
            target: "tddy_daemon::session_list_enrichment",
            "session_id {} not found in changeset sessions list; showing state/elapsed only",
            meta.session_id
        );
        return Ok(SessionListStatusDisplay {
            workflow_goal: "—".to_string(),
            workflow_state,
            elapsed_display,
            agent: "—".to_string(),
            model: "—".to_string(),
            activity_status: String::new(),
        });
    };

    let workflow_goal = row.tag.clone();
    let agent = row.agent.clone();
    let model =
        resolve_model_label_for_tag(&changeset, &row.tag).unwrap_or_else(|| "—".to_string());

    log::debug!(
        target: "tddy_daemon::session_list_enrichment",
        "enriched session {}: goal={} state={} agent={} model={} elapsed={}",
        meta.session_id,
        workflow_goal,
        workflow_state,
        agent,
        model,
        elapsed_display
    );

    Ok(SessionListStatusDisplay {
        workflow_goal,
        workflow_state,
        elapsed_display,
        agent,
        model,
        activity_status: String::new(),
    })
}

/// Maps `changeset.models[tag]` to the effective model label.
pub fn resolve_model_label_for_tag(changeset: &Changeset, tag: &str) -> Option<String> {
    let out = changeset.models.get(tag).cloned();
    log::debug!(
        target: "tddy_daemon::session_list_enrichment",
        "resolve_model_label_for_tag({tag:?}) -> {out:?}"
    );
    out
}

/// Copies enrichment display strings into a proto `SessionEntry` (daemon list path).
pub fn apply_session_list_status_to_proto(
    session_dir: &Path,
    entry: &mut ProtoSessionEntry,
) -> anyhow::Result<()> {
    log::debug!(
        target: "tddy_daemon::session_list_enrichment",
        "apply_session_list_status_to_proto session_id={}",
        entry.session_id
    );
    let status = session_list_status_from_session_dir(session_dir)?;
    entry.workflow_goal = status.workflow_goal;
    entry.workflow_state = status.workflow_state;
    entry.elapsed_display = status.elapsed_display;
    entry.agent = status.agent;
    entry.model = status.model;
    entry.activity_status = status.activity_status;
    entry.pending_elicitation =
        crate::elicitation::pending_elicitation_for_session_dir(session_dir);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use tddy_core::read_changeset;
    use tempfile::tempdir;

    #[test]
    fn list_sessions_enrichment_matches_changeset_fixture() {
        let dir = tempdir().unwrap();
        let session_dir = dir.path();
        fs::write(
            session_dir.join(tddy_core::SESSION_METADATA_FILENAME),
            r"session_id: fixture-sess-1
project_id: proj-1
created_at: '2026-03-28T10:00:00Z'
updated_at: '2026-03-28T12:00:00Z'
status: active
repo_path: /tmp/repo
pid: 42
",
        )
        .unwrap();
        fs::write(
            session_dir.join("changeset.yaml"),
            r"version: 1
models:
  acceptance-tests: sonnet-4
sessions:
  - id: fixture-sess-1
    agent: claude
    tag: acceptance-tests
    created_at: '2026-03-28T10:00:00Z'
state:
  current: Red
  session_id: fixture-sess-1
  updated_at: '2026-03-28T12:00:00Z'
  history:
    - state: Init
      at: '2026-03-28T11:00:00Z'
    - state: Red
      at: '2026-03-28T12:00:00Z'
",
        )
        .unwrap();

        let got = session_list_status_from_session_dir(session_dir).unwrap();
        assert_eq!(got.workflow_goal, "acceptance-tests");
        assert_eq!(got.workflow_state, "Red");
        assert_eq!(got.agent, "claude");
        assert_eq!(got.model, "sonnet-4");
        assert_ne!(got.elapsed_display, "—");
    }

    #[test]
    fn elapsed_display_for_changeset_matches_fixture_history() {
        let dir = tempdir().unwrap();
        let session_dir = dir.path();
        fs::write(
            session_dir.join(tddy_core::SESSION_METADATA_FILENAME),
            r"session_id: fixture-sess-elapsed
project_id: proj-1
created_at: '2026-03-28T10:00:00Z'
updated_at: '2026-03-28T12:00:00Z'
status: active
repo_path: /tmp/repo
pid: 42
",
        )
        .unwrap();
        fs::write(
            session_dir.join("changeset.yaml"),
            r"version: 1
models:
  acceptance-tests: sonnet-4
sessions:
  - id: fixture-sess-elapsed
    agent: claude
    tag: acceptance-tests
    created_at: '2026-03-28T10:00:00Z'
state:
  current: Red
  session_id: fixture-sess-elapsed
  updated_at: '2026-03-28T12:00:00Z'
  history:
    - state: Init
      at: '2026-03-28T11:00:00Z'
    - state: Red
      at: '2026-03-28T12:00:00Z'
",
        )
        .unwrap();
        let cs = read_changeset(session_dir).expect("fixture changeset");
        let got = elapsed_display_for_changeset(&cs);
        assert_ne!(got, "—");
        assert!(got.contains('m') || got.contains('s') || got.contains('h'));
    }

    #[test]
    fn resolve_model_label_maps_models_section_for_tag() {
        let dir = tempdir().unwrap();
        let session_dir = dir.path();
        fs::write(
            session_dir.join("changeset.yaml"),
            r"version: 1
models:
  acceptance-tests: sonnet-4
sessions: []
state:
  current: Init
  updated_at: '2026-03-28T10:00:00Z'
  history: []
",
        )
        .unwrap();
        let cs = read_changeset(session_dir).expect("fixture changeset");
        assert_eq!(
            resolve_model_label_for_tag(&cs, "acceptance-tests").as_deref(),
            Some("sonnet-4")
        );
    }

    #[test]
    fn apply_session_list_status_to_proto_updates_extended_fields() {
        let dir = tempdir().unwrap();
        let session_dir = dir.path();
        fs::write(
            session_dir.join(tddy_core::SESSION_METADATA_FILENAME),
            r"session_id: apply-proto-1
project_id: proj-1
created_at: '2026-03-28T10:00:00Z'
updated_at: '2026-03-28T12:00:00Z'
status: active
repo_path: /tmp/repo
pid: 0
",
        )
        .unwrap();
        fs::write(
            session_dir.join("changeset.yaml"),
            r"version: 1
models:
  acceptance-tests: sonnet-4
sessions:
  - id: apply-proto-1
    agent: claude
    tag: acceptance-tests
    created_at: '2026-03-28T10:00:00Z'
state:
  current: Red
  session_id: apply-proto-1
  updated_at: '2026-03-28T12:00:00Z'
  history:
    - state: Init
      at: '2026-03-28T11:00:00Z'
    - state: Red
      at: '2026-03-28T12:00:00Z'
",
        )
        .unwrap();

        let mut proto = ProtoSessionEntry {
            session_id: "apply-proto-1".to_string(),
            created_at: String::new(),
            status: String::new(),
            repo_path: String::new(),
            pid: 0,
            is_active: false,
            project_id: String::new(),
            daemon_instance_id: String::new(),
            workflow_goal: String::new(),
            workflow_state: String::new(),
            elapsed_display: String::new(),
            agent: String::new(),
            model: String::new(),
            pending_elicitation: false,
            activity_status: String::new(),
        };
        apply_session_list_status_to_proto(session_dir, &mut proto).unwrap();
        assert_eq!(proto.workflow_goal, "acceptance-tests");
        assert_eq!(proto.workflow_state, "Red");
        assert_eq!(proto.agent, "claude");
        assert_eq!(proto.model, "sonnet-4");
        assert_ne!(proto.elapsed_display, "—");
    }

    /// **claude_cli_session_enrichment_uses_metadata_not_changeset**: when `.session.yaml` has
    /// `session_type = "claude-cli"` and no `changeset.yaml` is present,
    /// `session_list_status_from_session_dir` must return `agent = "claude-cli"` and
    /// `model` from metadata — not `all_placeholders()` dashes.
    #[test]
    fn claude_cli_session_enrichment_uses_metadata_not_changeset() {
        use tddy_core::SessionMetadata;
        let dir = tempdir().unwrap();
        let session_dir = dir.path().join("sess-claude-cli-enrich-1");
        fs::create_dir_all(&session_dir).unwrap();

        // Write .session.yaml with session_type and model — new fields, compile error until added.
        let metadata = SessionMetadata {
            session_id: "sess-claude-cli-enrich-1".to_string(),
            project_id: "proj-x".to_string(),
            created_at: "2026-06-06T10:00:00Z".to_string(),
            updated_at: "2026-06-06T10:00:00Z".to_string(),
            status: "active".to_string(),
            repo_path: Some("/tmp/worktrees/claude-cli-abc".to_string()),
            pid: Some(12345),
            tool: None,
            livekit_room: None,
            pending_elicitation: false,
            previous_session_id: None,
            session_type: Some("claude-cli".to_string()),
            model: Some("claude-opus-4-8".to_string()),
            activity_status: None,
            hook_token: None,
        };
        tddy_core::write_session_metadata(&session_dir, &metadata).unwrap();
        // Intentionally NO changeset.yaml — claude-cli sessions never have one.

        let got = session_list_status_from_session_dir(&session_dir)
            .expect("enrichment must not error for claude-cli session without changeset");

        assert_eq!(
            got.agent, "claude-cli",
            "agent must be 'claude-cli' read from session_type metadata, not '—'"
        );
        assert_eq!(
            got.model, "claude-opus-4-8",
            "model must be read from .session.yaml model field, not '—'"
        );
        assert!(
            got.workflow_goal.is_empty() || got.workflow_goal == "—",
            "workflow_goal must be empty/dash for sessions without changeset; got: {}",
            got.workflow_goal
        );
        assert!(
            got.workflow_state.is_empty() || got.workflow_state == "—",
            "workflow_state must be empty/dash for sessions without changeset; got: {}",
            got.workflow_state
        );
    }

    /// `claude_cli_enrichment_surfaces_activity_status_from_metadata` — when `.session.yaml` has
    /// `activity_status = "WaitingForInput"` for a claude-cli session, the enrichment function
    /// must return that string in `SessionListStatusDisplay.activity_status`.
    #[test]
    fn claude_cli_enrichment_surfaces_activity_status_from_metadata() {
        use tddy_core::SessionMetadata;
        let dir = tempdir().unwrap();
        let session_dir = dir.path().join("sess-act-enrich-1");
        fs::create_dir_all(&session_dir).unwrap();

        let metadata = SessionMetadata {
            session_id: "sess-act-enrich-1".to_string(),
            project_id: "proj-x".to_string(),
            created_at: "2026-06-13T10:00:00Z".to_string(),
            updated_at: "2026-06-13T10:05:00Z".to_string(),
            status: "active".to_string(),
            repo_path: Some("/tmp/worktrees/claude-cli-act".to_string()),
            pid: Some(11111),
            tool: None,
            livekit_room: None,
            pending_elicitation: false,
            previous_session_id: None,
            session_type: Some("claude-cli".to_string()),
            model: Some("claude-sonnet-4-6".to_string()),
            activity_status: Some("WaitingForInput".to_string()),
            hook_token: None,
        };
        tddy_core::write_session_metadata(&session_dir, &metadata).unwrap();

        let got =
            session_list_status_from_session_dir(&session_dir).expect("enrichment must not error");

        assert_eq!(
            got.activity_status, "WaitingForInput",
            "activity_status must be read from .session.yaml"
        );
        assert_eq!(got.agent, "claude-cli");
    }

    /// `claude_cli_enrichment_empty_activity_status_when_absent` — when `.session.yaml` has no
    /// `activity_status`, the enrichment must return an empty string (not an error or a dash).
    #[test]
    fn claude_cli_enrichment_empty_activity_status_when_absent() {
        use tddy_core::SessionMetadata;
        let dir = tempdir().unwrap();
        let session_dir = dir.path().join("sess-act-absent-1");
        fs::create_dir_all(&session_dir).unwrap();

        let metadata = SessionMetadata {
            session_id: "sess-act-absent-1".to_string(),
            project_id: "proj-x".to_string(),
            created_at: "2026-06-13T10:00:00Z".to_string(),
            updated_at: "2026-06-13T10:00:00Z".to_string(),
            status: "active".to_string(),
            repo_path: Some("/tmp/wt".to_string()),
            pid: None,
            tool: None,
            livekit_room: None,
            pending_elicitation: false,
            previous_session_id: None,
            session_type: Some("claude-cli".to_string()),
            model: Some("claude-opus-4-8".to_string()),
            activity_status: None,
            hook_token: None,
        };
        tddy_core::write_session_metadata(&session_dir, &metadata).unwrap();

        let got =
            session_list_status_from_session_dir(&session_dir).expect("enrichment must not error");

        assert_eq!(
            got.activity_status, "",
            "activity_status must be empty string when absent in .session.yaml"
        );
    }

    /// `tool_session_enrichment_has_empty_activity_status` — tool/workflow sessions must always
    /// have `activity_status = ""` (never populated from changeset data).
    #[test]
    fn tool_session_enrichment_has_empty_activity_status() {
        let dir = tempdir().unwrap();
        let session_dir = dir.path().join("sess-tool-act-1");
        fs::create_dir_all(&session_dir).unwrap();

        // Write a standard tool session + changeset
        use tddy_core::SessionMetadata;
        let metadata = SessionMetadata {
            session_id: "sess-tool-act-1".to_string(),
            project_id: "proj-t".to_string(),
            created_at: "2026-06-13T10:00:00Z".to_string(),
            updated_at: "2026-06-13T10:00:00Z".to_string(),
            status: "active".to_string(),
            repo_path: Some("/tmp/repo".to_string()),
            pid: Some(22222),
            tool: Some("tddy-coder".to_string()),
            livekit_room: None,
            pending_elicitation: false,
            previous_session_id: None,
            session_type: None,
            model: None,
            activity_status: None,
            hook_token: None,
        };
        tddy_core::write_session_metadata(&session_dir, &metadata).unwrap();

        std::fs::write(
            session_dir.join("changeset.yaml"),
            r"version: 1
models:
  acceptance-tests: sonnet-4
state:
  current: Red
  updated_at: '2026-06-13T10:00:00Z'
  history:
    - state: Red
      at: '2026-06-13T10:00:00Z'
sessions:
  - tag: acceptance-tests
    agent: claude
    session_id: sess-tool-act-1
",
        )
        .unwrap();

        let got = session_list_status_from_session_dir(&session_dir)
            .expect("enrichment must not error for tool session");

        assert_eq!(
            got.activity_status, "",
            "tool sessions must have empty activity_status"
        );
    }

    /// `apply_session_list_status_to_proto_sets_activity_status` — when a claude-cli session has
    /// `activity_status` in `.session.yaml`, `apply_session_list_status_to_proto` must copy it
    /// to `proto.activity_status`. Tool/changeset sessions must produce an empty string.
    /// `entry.activity_status`.
    #[test]
    fn apply_session_list_status_to_proto_sets_activity_status() {
        use tddy_core::SessionMetadata;
        let dir = tempdir().unwrap();
        let session_dir = dir.path().join("sess-proto-act-1");
        fs::create_dir_all(&session_dir).unwrap();

        let metadata = SessionMetadata {
            session_id: "sess-proto-act-1".to_string(),
            project_id: "proj-z".to_string(),
            created_at: "2026-06-13T10:00:00Z".to_string(),
            updated_at: "2026-06-13T10:00:00Z".to_string(),
            status: "active".to_string(),
            repo_path: Some("/tmp/wt-proto".to_string()),
            pid: None,
            tool: None,
            livekit_room: None,
            pending_elicitation: false,
            previous_session_id: None,
            session_type: Some("claude-cli".to_string()),
            model: Some("claude-sonnet-4-6".to_string()),
            activity_status: Some("Done".to_string()),
            hook_token: None,
        };
        tddy_core::write_session_metadata(&session_dir, &metadata).unwrap();

        let mut proto = ProtoSessionEntry {
            session_id: "sess-proto-act-1".to_string(),
            created_at: String::new(),
            status: String::new(),
            repo_path: String::new(),
            pid: 0,
            is_active: false,
            project_id: String::new(),
            daemon_instance_id: String::new(),
            workflow_goal: String::new(),
            workflow_state: String::new(),
            elapsed_display: String::new(),
            agent: String::new(),
            model: String::new(),
            pending_elicitation: false,
            activity_status: String::new(),
        };
        apply_session_list_status_to_proto(&session_dir, &mut proto).unwrap();
        assert_eq!(
            proto.activity_status, "Done",
            "activity_status must be propagated from .session.yaml into proto SessionEntry"
        );
        // agent field must also be populated for claude-cli
        assert_eq!(proto.agent, "claude-cli");
    }

    #[test]
    fn pending_elicitation_for_session_dir_reads_metadata_flag() {
        use tddy_core::SessionMetadata;
        let dir = tempdir().unwrap();
        let session_dir = dir.path().join("sess-pe-unit-1");
        fs::create_dir_all(&session_dir).unwrap();
        let metadata = SessionMetadata {
            session_id: "sess-pe-unit-1".to_string(),
            project_id: "proj-1".to_string(),
            created_at: "2026-03-28T10:00:00Z".to_string(),
            updated_at: "2026-03-28T12:00:00Z".to_string(),
            status: "active".to_string(),
            repo_path: None,
            pid: None,
            tool: None,
            livekit_room: None,
            pending_elicitation: true,
            previous_session_id: None,
            session_type: None,
            model: None,
            activity_status: None,
            hook_token: None,
        };
        tddy_core::write_session_metadata(&session_dir, &metadata).unwrap();
        assert!(
            crate::elicitation::pending_elicitation_for_session_dir(&session_dir),
            "pending_elicitation in .session.yaml must map to the Connection list flag"
        );
    }
}
