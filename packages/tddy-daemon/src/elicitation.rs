//! Pending elicitation: shared definition for Telegram notifications and Connection `ListSessions`.
//!
//! **Authoritative list flag**: [`pending_elicitation_for_session_dir`] reads `pending_elicitation` from
//! [`.session.yaml`](tddy_core::SESSION_METADATA_FILENAME) (see [`tddy_core::SessionMetadata`]), written
//! by the tool when elicitation starts/ends.
//!
//! **Presenter stream**: [`telegram_elicitation_line_for_mode_changed`] treats [`ModeChanged`] as
//! elicitation when the presenter [`AppMode`](tddy_service::gen::app_mode_proto) requires user input
//! or approval: document review, markdown viewer, feature input, clarification select/multi-select,
//! and free-text input. Autonomous modes (`Running`, `Done`) are not elicitation.

use std::path::Path;

use tddy_service::gen::app_mode_proto;
use tddy_service::gen::ModeChanged;

/// Stable dedupe key for identical [`ModeChanged`] payloads (Telegram anti-spam).
pub fn elicitation_signature_for_mode_changed(mc: &ModeChanged) -> String {
    let mut out = String::from("mode_changed:v1:");
    let Some(ref mode) = mc.mode else {
        out.push_str("no_mode");
        log::debug!(
            target: "tddy_daemon::elicitation",
            "elicitation_signature_for_mode_changed: empty AppModeProto"
        );
        return out;
    };
    let Some(ref v) = mode.variant else {
        out.push_str("no_variant");
        log::debug!(
            target: "tddy_daemon::elicitation",
            "elicitation_signature_for_mode_changed: empty variant"
        );
        return out;
    };
    use app_mode_proto::Variant;
    match v {
        Variant::DocumentReview(d) => {
            out.push_str("document_review:");
            out.push_str(&d.content);
        }
        Variant::MarkdownViewer(d) => {
            out.push_str("markdown_viewer:");
            out.push_str(&d.content);
        }
        Variant::FeatureInput(_) => out.push_str("feature_input"),
        Variant::Running(_) => out.push_str("running"),
        Variant::Done(_) => out.push_str("done"),
        Variant::Select(s) => {
            out.push_str("select:");
            out.push_str(&s.question_index.to_string());
            out.push(':');
            out.push_str(&s.total_questions.to_string());
            out.push(':');
            if let Some(q) = s.question.as_ref() {
                out.push_str(&q.header);
                out.push('|');
                out.push_str(&q.question);
            }
        }
        Variant::MultiSelect(m) => {
            out.push_str("multi_select:");
            out.push_str(&m.question_index.to_string());
            out.push(':');
            out.push_str(&m.total_questions.to_string());
            out.push(':');
            if let Some(q) = m.question.as_ref() {
                out.push_str(&q.header);
                out.push('|');
                out.push_str(&q.question);
            }
        }
        Variant::TextInput(t) => {
            out.push_str("text_input:");
            out.push_str(&t.prompt);
        }
    }
    log::debug!(
        target: "tddy_daemon::elicitation",
        "elicitation_signature_for_mode_changed: len={} (prefix only in logs)",
        out.len()
    );
    out
}

/// Telegram line when the presenter signals user-input / approval elicitation.
///
/// Returns [`None`] for autonomous modes (`Running`, `Done`) so operators are not pinged while the
/// agent is running without a gate.
pub fn telegram_elicitation_line_for_mode_changed(
    session_label: &str,
    mc: &ModeChanged,
) -> Option<String> {
    let Some(ref mode) = mc.mode else {
        log::debug!(
            target: "tddy_daemon::elicitation",
            "telegram_elicitation_line_for_mode_changed: no AppModeProto"
        );
        return None;
    };
    let Some(ref v) = mode.variant else {
        log::debug!(
            target: "tddy_daemon::elicitation",
            "telegram_elicitation_line_for_mode_changed: no variant"
        );
        return None;
    };

    use app_mode_proto::Variant;
    let line = match v {
        Variant::DocumentReview(_) => format!(
            "Session {session_label}: approval needed — review the session document before continuing"
        ),
        Variant::MarkdownViewer(_) => format!(
            "Session {session_label}: input needed — review the document in the UI before continuing"
        ),
        Variant::FeatureInput(_) => format!(
            "Session {session_label}: input needed — feature input is required to continue"
        ),
        Variant::Select(_) | Variant::MultiSelect(_) => format!(
            "Session {session_label}: input needed — answer the clarification prompt to continue"
        ),
        Variant::TextInput(_) => format!(
            "Session {session_label}: input needed — provide text input to continue"
        ),
        Variant::Running(_) | Variant::Done(_) => {
            log::debug!(
                target: "tddy_daemon::elicitation",
                "telegram_elicitation_line_for_mode_changed: non-elicitation mode (running/done)"
            );
            return None;
        }
    };

    log::info!(
        target: "tddy_daemon::elicitation",
        "telegram_elicitation_line_for_mode_changed: session_label_len={} text_len={}",
        session_label.len(),
        line.len()
    );
    Some(line)
}

/// Reads `pending_elicitation` from `.session.yaml` for the Connection session list.
pub fn pending_elicitation_for_session_dir(session_dir: &Path) -> bool {
    match tddy_core::read_session_metadata(session_dir) {
        Ok(meta) => {
            log::debug!(
                target: "tddy_daemon::elicitation",
                "pending_elicitation_for_session_dir: session_id={} pending_elicitation={}",
                meta.session_id,
                meta.pending_elicitation
            );
            meta.pending_elicitation
        }
        Err(e) => {
            log::debug!(
                target: "tddy_daemon::elicitation",
                "pending_elicitation_for_session_dir: no metadata in {}: {e}",
                session_dir.display()
            );
            false
        }
    }
}
