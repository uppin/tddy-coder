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
        };
        apply_session_list_status_to_proto(session_dir, &mut proto).unwrap();
        assert_eq!(proto.workflow_goal, "acceptance-tests");
        assert_eq!(proto.workflow_state, "Red");
        assert_eq!(proto.agent, "claude");
        assert_eq!(proto.model, "sonnet-4");
        assert_ne!(proto.elapsed_display, "—");
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
        };
        tddy_core::write_session_metadata(&session_dir, &metadata).unwrap();
        assert!(
            crate::elicitation::pending_elicitation_for_session_dir(&session_dir),
            "pending_elicitation in .session.yaml must map to the Connection list flag"
        );
    }
}
