//! What the session records about the agent: its own tool calls, persisted and broadcast, and
//! the agent-output lines folded into the activity log. Also the progress and worktree handlers
//! `poll_workflow` dispatches here — `on_progress` and `on_worktree_switched` — which keep the
//! status bar and the worktree display current.

use std::path::PathBuf;

use super::{format_session_id_for_log, now_unix_ms, Presenter};
use crate::presenter::agent_activity;
use crate::presenter::presenter_events::PresenterEvent;
use crate::presenter::state::{ActivityEntry, ActivityKind};
use crate::presenter::worktree_display::format_worktree_for_status_bar;

impl Presenter {
    /// The commit this session's checkout is on, or empty when this presenter was never told which
    /// checkout that is.
    ///
    /// Empty is the documented "could not resolve" value (AC1). Reading some *other* directory's
    /// HEAD — the session dir, the process's own cwd — would answer with a real sha for the wrong
    /// tree, which is the fabrication AC1 forbids in its least obvious form: a mirror would apply a
    /// change onto a base it was never cut from and report success.
    fn agent_activity_head_commit(&self) -> String {
        match self.activity.worktree.as_deref() {
            Some(worktree) => crate::git_head::read_head_commit(worktree),
            None => String::new(),
        }
    }

    /// The worktree paths a call declared it would write, relative to this session's checkout.
    ///
    /// Empty without a checkout, because these paths exist only as a relation to one: a pathspec is
    /// relative to a repository root, and with no root there is nothing to express them against.
    fn agent_activity_declared_paths(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> Vec<String> {
        match self.activity.worktree.as_deref() {
            Some(worktree) => crate::agent_activity::declared_paths(tool_name, input, worktree),
            None => Vec::new(),
        }
    }

    /// Persist and broadcast the agent's own tool call for a `ToolUse` / `ToolResult` progress
    /// event. No-op unless [`set_agent_activity_context`](Self::set_agent_activity_context) wired a
    /// session dir (i.e. tool / cursor-cli sessions, where the coder executes tools). Never panics:
    /// a persistence failure is logged and the live broadcast still fires (TUI-safe — no
    /// `println!`/`eprintln!`).
    fn capture_agent_activity(&mut self, pev: &crate::ProgressEvent) {
        use crate::agent_activity::{
            append_agent_activity, AgentActivityRecord, STATUS_COMPLETED, STATUS_ERROR,
            STATUS_RUNNING,
        };
        let dir = match self.activity.dir.clone() {
            Some(d) => d,
            None => return,
        };
        let record = match pev {
            crate::ProgressEvent::ToolUse {
                name,
                input_json,
                call_id,
                ..
            } => {
                let call_id = call_id
                    .clone()
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                let input = crate::agent_activity::parse_activity_json(
                    &input_json.clone().unwrap_or_default(),
                );
                let rec = AgentActivityRecord {
                    call_id: call_id.clone(),
                    tool_name: name.clone(),
                    // Stamped before the call runs, which is the point: this is the state the call
                    // is about to change, so a consumer applying its delta knows what it applies
                    // onto (AC1). Reading HEAD is a couple of file reads rather than a subprocess,
                    // which is what makes it affordable on every tool call.
                    head_commit: self.agent_activity_head_commit(),
                    changed_paths: self.agent_activity_declared_paths(name, &input),
                    input,
                    status: STATUS_RUNNING.to_string(),
                    result: serde_json::Value::Null,
                    error_message: String::new(),
                    started_unix_ms: now_unix_ms(),
                    completed_unix_ms: 0,
                    source: self.activity.source.clone(),
                    // The poll tick covering this call is measured by the session room, which is a
                    // different process; 0 is its documented "no tick has covered it yet" (AC2).
                    activity_seq: 0,
                };
                self.activity.pending.insert(call_id, rec.clone());
                rec
            }
            crate::ProgressEvent::ToolResult {
                call_id,
                result_json,
                is_error,
            } => {
                // Base the terminal row on the remembered running row so the coalesced record keeps
                // the tool name / input / start time — and the commit that row was stamped with,
                // which is the state the call ran upon rather than the one it left behind. Falls
                // back to a minimal row if no running row was seen (e.g. the presenter attached
                // mid-call).
                let pending = self.activity.pending.remove(call_id);
                // Only for that fallback, and read here because there is nothing to inherit: this
                // row is the first this presenter saw of the call, so the moment it is recorded is
                // now, and now is the HEAD it can honestly name. It declares no paths, having never
                // seen the tool's input.
                let mut rec = pending.unwrap_or_else(|| AgentActivityRecord {
                    call_id: call_id.clone(),
                    tool_name: String::new(),
                    input: serde_json::Value::Null,
                    status: String::new(),
                    result: serde_json::Value::Null,
                    error_message: String::new(),
                    started_unix_ms: 0,
                    completed_unix_ms: 0,
                    source: self.activity.source.clone(),
                    head_commit: self.agent_activity_head_commit(),
                    activity_seq: 0,
                    changed_paths: Vec::new(),
                });
                rec.status = if *is_error {
                    STATUS_ERROR
                } else {
                    STATUS_COMPLETED
                }
                .to_string();
                rec.result = crate::agent_activity::parse_activity_json(result_json);
                rec.error_message = if *is_error {
                    result_json.clone()
                } else {
                    String::new()
                };
                rec.completed_unix_ms = now_unix_ms();
                rec
            }
            _ => return,
        };
        if let Err(e) = append_agent_activity(&dir, &record) {
            log::warn!(
                target: "tddy_core::presenter",
                "agent_activity: failed to persist tool call to {}: {}",
                dir.display(),
                e
            );
        }
        self.broadcast(PresenterEvent::AgentActivity(record));
    }

    /// Completes a full agent line in `activity_log` after a newline (no `ActivityLogged` broadcast;
    /// streaming consumers use [`PresenterEvent::AgentOutput`]).
    fn finalize_agent_line_in_activity_log(&mut self, line: String) {
        if line.is_empty() {
            return;
        }
        log::debug!(
            "finalize_agent_line_in_activity_log: len={}, partial_row_active={}",
            line.len(),
            self.activity.output_partial_row_active
        );
        if self.activity.output_partial_row_active {
            if let Some(last) = self.state.activity_log.last_mut() {
                if last.kind == ActivityKind::AgentOutput {
                    last.text = line;
                    self.activity.output_partial_row_active = false;
                    return;
                }
            }
            self.activity.output_partial_row_active = false;
        }
        self.state.activity_log.push(ActivityEntry {
            text: line,
            kind: ActivityKind::AgentOutput,
        });
    }

    /// Syncs the visible tail of the current incomplete agent line into `activity_log` (incremental).
    fn sync_agent_partial_activity_log(&mut self) {
        let tail = agent_activity::visible_tail_for_incremental_log(&self.activity.output_buffer);
        if tail.is_empty() {
            return;
        }
        log::debug!(
            "sync_agent_partial_activity_log: tail_len={}, partial_row_active={}",
            tail.len(),
            self.activity.output_partial_row_active
        );
        if self.activity.output_partial_row_active {
            if let Some(last) = self.state.activity_log.last_mut() {
                if last.kind == ActivityKind::AgentOutput {
                    last.text = tail;
                    return;
                }
            }
        }
        self.state.activity_log.push(ActivityEntry {
            text: tail,
            kind: ActivityKind::AgentOutput,
        });
        self.activity.output_partial_row_active = true;
    }

    /// Capture and log one progress event from the workflow.
    pub(super) fn on_progress(&mut self, pev: crate::ProgressEvent) {
        if let crate::ProgressEvent::SessionStarted { session_id } = &pev {
            log::info!("Workflow engine session started; TUI status segment will use id prefix");
            log::debug!(
                "workflow_session_id set for status bar: {}",
                format_session_id_for_log(session_id)
            );
            self.state.workflow_session_id = Some(session_id.clone());
        }
        // Persist + broadcast the agent's own tool call (tool / cursor-cli sessions).
        self.capture_agent_activity(&pev);
        let entry = match &pev {
            crate::ProgressEvent::ToolUse {
                name,
                detail: Some(d),
                ..
            } => ActivityEntry {
                text: format!("Tool: {} {}", name, d),
                kind: ActivityKind::ToolUse,
            },
            crate::ProgressEvent::ToolUse {
                name, detail: None, ..
            } => ActivityEntry {
                text: format!("Tool: {}", name),
                kind: ActivityKind::ToolUse,
            },
            // A tool result is captured as agent activity above; it is not rendered as a
            // separate activity-log line, so skip building an entry for it.
            crate::ProgressEvent::ToolResult { .. } => return,
            crate::ProgressEvent::TaskStarted { description } => ActivityEntry {
                text: description.clone(),
                kind: ActivityKind::TaskStarted,
            },
            crate::ProgressEvent::TaskProgress { description, .. } => ActivityEntry {
                text: description.clone(),
                kind: ActivityKind::TaskProgress,
            },
            crate::ProgressEvent::SessionStarted { .. } => ActivityEntry {
                text: "Session connected".to_string(),
                kind: ActivityKind::Info,
            },
            crate::ProgressEvent::AgentExited { exit_code, goal } => ActivityEntry {
                text: format!("Agent exited (code {}) for {}", exit_code, goal),
                kind: ActivityKind::Info,
            },
        };
        self.state.activity_log.push(entry.clone());
        self.broadcast(PresenterEvent::ActivityLogged(entry));
    }

    /// Log the worktree the workflow switched to and show it in the status bar.
    pub(super) fn on_worktree_switched(&mut self, path: PathBuf) {
        let entry = ActivityEntry {
            text: format!("Worktree: {}", path.display()),
            kind: ActivityKind::Info,
        };
        self.state.activity_log.push(entry.clone());
        self.broadcast(PresenterEvent::ActivityLogged(entry));
        let wtd = format_worktree_for_status_bar(path.as_path());
        if !wtd.is_empty() {
            log::info!("WorktreeSwitched: active_worktree_display set to {:?}", wtd);
            self.state.active_worktree_display = Some(wtd);
        } else {
            log::debug!(
                "WorktreeSwitched: format_worktree_for_status_bar returned empty for {:?}",
                path
            );
        }
    }

    /// Fold an agent-output chunk into the activity log and forward it to the views.
    pub(super) fn on_agent_output(&mut self, text: String) {
        agent_activity::on_agent_chunk_received(&text);
        let channels = agent_activity::authoritative_channels_per_completed_line();
        log::info!(
            "poll_workflow: AgentOutput chunk len={}, policy_authoritative_channels={}",
            text.len(),
            channels
        );
        for part in text.split_inclusive('\n') {
            if part.ends_with('\n') {
                self.activity
                    .output_buffer
                    .push_str(part.trim_end_matches('\n'));
                let line = std::mem::take(&mut self.activity.output_buffer);
                if !line.is_empty() {
                    self.finalize_agent_line_in_activity_log(line);
                }
            } else {
                self.activity.output_buffer.push_str(part);
            }
        }
        self.sync_agent_partial_activity_log();
        self.broadcast(PresenterEvent::AgentOutput(text.clone()));
    }
}
