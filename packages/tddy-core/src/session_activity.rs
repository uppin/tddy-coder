//! Granular session activity status driven by per-worktree Claude Code hooks.
//!
//! The daemon writes a `.claude/settings.local.json` into each claude-cli worktree wiring six
//! hook events to `tddy-tools session-hook`. That command reads the hook event JSON from stdin,
//! calls [`activity_status_from_hook`] to get a status, and POSTs it to the daemon via
//! [`ReportSessionStatus`] gRPC. This module owns the pure, side-effect-free mapping and parse
//! functions — shared between the reporter (tddy-tools) and the receiver (tddy-daemon).

use serde::Deserialize;

// ---------------------------------------------------------------------------
// Status enum
// ---------------------------------------------------------------------------

/// Granular activity status for a claude-cli session, derived from Claude Code hook events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionActivityStatus {
    /// `SessionStart` — session has just been started or resumed.
    Started,
    /// `UserPromptSubmit` — user sent a prompt; Claude is processing.
    Running,
    /// `PostToolUse` — a tool call completed; still in the agentic loop.
    ExecutingTool,
    /// `Notification` (permission_prompt / elicitation_dialog / idle_prompt) — waiting on the
    /// user to approve a tool, answer a question, or re-engage.
    WaitingForInput,
    /// `Stop` — Claude finished responding for this turn.
    Done,
    /// `SessionEnd` — session terminated.
    Ended,
}

impl SessionActivityStatus {
    /// Canonical wire string used in the gRPC request and `.session.yaml`.
    ///
    /// These strings are stable — changing them is a breaking change for persisted YAML and any
    /// dashboard that reads `activity_status`.
    pub fn as_wire(self) -> &'static str {
        match self {
            SessionActivityStatus::Started => "Started",
            SessionActivityStatus::Running => "Running",
            SessionActivityStatus::ExecutingTool => "ExecutingTool",
            SessionActivityStatus::WaitingForInput => "WaitingForInput",
            SessionActivityStatus::Done => "Done",
            SessionActivityStatus::Ended => "Ended",
        }
    }

    /// Parse a wire string back into a `SessionActivityStatus`.
    ///
    /// Returns `None` for unknown strings — callers should reject the request with
    /// `InvalidArgument`.
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "Started" => Some(SessionActivityStatus::Started),
            "Running" => Some(SessionActivityStatus::Running),
            "ExecutingTool" => Some(SessionActivityStatus::ExecutingTool),
            "WaitingForInput" => Some(SessionActivityStatus::WaitingForInput),
            "Done" => Some(SessionActivityStatus::Done),
            "Ended" => Some(SessionActivityStatus::Ended),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Hook event → status mapping
// ---------------------------------------------------------------------------

/// Map a Claude Code hook event to a granular [`SessionActivityStatus`].
///
/// Returns `None` for events that are not relevant to session activity (no-op: the caller should
/// exit 0 without contacting the daemon).
///
/// `hook_event_name` is the `hook_event_name` field from the Claude Code hook stdin JSON.
/// `notification_type` is the `notification_type` field, present only for `Notification` events.
pub fn activity_status_from_hook(
    hook_event_name: &str,
    notification_type: Option<&str>,
) -> Option<SessionActivityStatus> {
    match hook_event_name {
        "SessionStart" => Some(SessionActivityStatus::Started),
        "UserPromptSubmit" => Some(SessionActivityStatus::Running),
        "PostToolUse" => Some(SessionActivityStatus::ExecutingTool),
        "Notification" => match notification_type {
            Some("permission_prompt") | Some("elicitation_dialog") | Some("idle_prompt") => {
                Some(SessionActivityStatus::WaitingForInput)
            }
            _ => None,
        },
        "Stop" => Some(SessionActivityStatus::Done),
        "SessionEnd" => Some(SessionActivityStatus::Ended),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Hook event JSON parsing
// ---------------------------------------------------------------------------

/// Parsed subset of the Claude Code hook stdin JSON payload.
///
/// Unknown fields (e.g. `transcript_path`, `message`, `tool_name`, `tool_input`) are ignored by
/// serde — safe because Claude Code may add fields per event type.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HookEvent {
    pub hook_event_name: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    /// Present only for `Notification` events. Distinguishes permission_prompt /
    /// elicitation_dialog / idle_prompt / etc.
    #[serde(default)]
    pub notification_type: Option<String>,
}

impl HookEvent {
    /// Convenience: derive the activity status for this event.
    pub fn activity_status(&self) -> Option<SessionActivityStatus> {
        activity_status_from_hook(&self.hook_event_name, self.notification_type.as_deref())
    }
}

/// Parse the Claude Code hook event JSON delivered on stdin.
pub fn parse_hook_event(stdin_json: &str) -> Result<HookEvent, serde_json::Error> {
    serde_json::from_str(stdin_json)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- Status mapping (one test per table row, happy paths first) ----------

    /// `SessionStart` maps to `Started`.
    #[test]
    fn session_start_event_maps_to_started() {
        // When
        let status = activity_status_from_hook("SessionStart", None);

        // Then
        assert_eq!(status, Some(SessionActivityStatus::Started));
    }

    /// `UserPromptSubmit` maps to `Running`.
    #[test]
    fn user_prompt_submit_maps_to_running() {
        // When
        let status = activity_status_from_hook("UserPromptSubmit", None);

        // Then
        assert_eq!(status, Some(SessionActivityStatus::Running));
    }

    /// `PostToolUse` maps to `ExecutingTool`.
    #[test]
    fn post_tool_use_maps_to_executing_tool() {
        // When
        let status = activity_status_from_hook("PostToolUse", None);

        // Then
        assert_eq!(status, Some(SessionActivityStatus::ExecutingTool));
    }

    /// `Notification` with `notification_type = permission_prompt` → `WaitingForInput`.
    #[test]
    fn notification_permission_prompt_maps_to_waiting_for_input() {
        // When
        let status = activity_status_from_hook("Notification", Some("permission_prompt"));

        // Then
        assert_eq!(status, Some(SessionActivityStatus::WaitingForInput));
    }

    /// `Notification` with `notification_type = elicitation_dialog` → `WaitingForInput`.
    #[test]
    fn notification_elicitation_dialog_maps_to_waiting_for_input() {
        // When
        let status = activity_status_from_hook("Notification", Some("elicitation_dialog"));

        // Then
        assert_eq!(status, Some(SessionActivityStatus::WaitingForInput));
    }

    /// `Notification` with `notification_type = idle_prompt` → `WaitingForInput`.
    #[test]
    fn notification_idle_prompt_maps_to_waiting_for_input() {
        // When
        let status = activity_status_from_hook("Notification", Some("idle_prompt"));

        // Then
        assert_eq!(status, Some(SessionActivityStatus::WaitingForInput));
    }

    /// `Stop` maps to `Done`.
    #[test]
    fn stop_event_maps_to_done() {
        // When
        let status = activity_status_from_hook("Stop", None);

        // Then
        assert_eq!(status, Some(SessionActivityStatus::Done));
    }

    /// `SessionEnd` maps to `Ended`.
    #[test]
    fn session_end_maps_to_ended() {
        // When
        let status = activity_status_from_hook("SessionEnd", None);

        // Then
        assert_eq!(status, Some(SessionActivityStatus::Ended));
    }

    /// Unknown events (e.g. `PreToolUse`) return `None` — no-op, no daemon call.
    #[test]
    fn unknown_event_is_noop() {
        // When
        let status = activity_status_from_hook("PreToolUse", None);

        // Then
        assert_eq!(status, None, "unrecognised hook event should produce no status");
    }

    /// `Notification` with an unknown subtype (e.g. `banner`) returns `None` — no-op.
    #[test]
    fn notification_unknown_subtype_is_noop() {
        // When
        let status = activity_status_from_hook("Notification", Some("banner"));

        // Then
        assert_eq!(status, None, "unknown notification subtype should produce no status");
    }

    /// `as_wire()` returns the stable strings that the daemon and web UI depend on.
    #[test]
    fn wire_strings_are_stable() {
        // Then
        assert_eq!(SessionActivityStatus::Started.as_wire(), "Started");
        assert_eq!(SessionActivityStatus::Running.as_wire(), "Running");
        assert_eq!(SessionActivityStatus::ExecutingTool.as_wire(), "ExecutingTool");
        assert_eq!(SessionActivityStatus::WaitingForInput.as_wire(), "WaitingForInput");
        assert_eq!(SessionActivityStatus::Done.as_wire(), "Done");
        assert_eq!(SessionActivityStatus::Ended.as_wire(), "Ended");
    }

    // --- stdin JSON parsing --------------------------------------------------

    /// Parse a `SessionStart` hook event with full fields; unknown fields are ignored.
    #[test]
    fn parses_session_start_event() {
        // Given
        let json = r#"{"hook_event_name":"SessionStart","session_id":"abc123","cwd":"/repo","transcript_path":"/tmp/t.jsonl"}"#;

        // When
        let ev = parse_hook_event(json).expect("must parse SessionStart event");

        // Then
        assert_eq!(ev.hook_event_name, "SessionStart");
        assert_eq!(ev.session_id.as_deref(), Some("abc123"));
        assert_eq!(ev.cwd.as_deref(), Some("/repo"));
        assert_eq!(ev.notification_type, None);
    }

    /// Parse a `Notification` event; `notification_type` must be captured.
    #[test]
    fn parses_notification_event_with_type() {
        // Given
        let json = r#"{"hook_event_name":"Notification","notification_type":"permission_prompt","session_id":"x","cwd":"/r","message":"run: Bash"}"#;

        // When
        let ev = parse_hook_event(json).expect("must parse Notification event");

        // Then
        assert_eq!(ev.hook_event_name, "Notification");
        assert_eq!(ev.notification_type.as_deref(), Some("permission_prompt"));
    }

    /// End-to-end: parse a notification event then derive activity status via the convenience
    /// method — proves `HookEvent::activity_status()` wires parse → map correctly.
    #[test]
    fn parsed_notification_event_maps_to_waiting_for_input() {
        // Given
        let json = r#"{"hook_event_name":"Notification","notification_type":"permission_prompt","session_id":"s1","cwd":"/w"}"#;

        // When
        let ev = parse_hook_event(json).unwrap();

        // Then
        assert_eq!(ev.activity_status(), Some(SessionActivityStatus::WaitingForInput));
    }

    /// `parse_hook_event` rejects JSON that is missing the required `hook_event_name` field.
    #[test]
    fn parse_rejects_missing_hook_event_name() {
        // Given
        let json = r#"{"session_id":"s","cwd":"/r"}"#;

        // When
        let result = parse_hook_event(json);

        // Then
        assert!(result.is_err(), "missing hook_event_name must be a parse error");
    }
}
