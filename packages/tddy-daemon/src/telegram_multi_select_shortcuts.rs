//! Compact MultiSelect inline shortcuts for Telegram clarification (`eli:mn:` / `eli:mr:`).

use crate::telegram_notifier::InlineKeyboardRows;

pub const CHOOSE_NONE_CB_PREFIX: &str = "eli:mn:";
pub const CHOOSE_RECOMMENDED_CB_PREFIX: &str = "eli:mr:";

/// Callback wire for Tap **Choose none**: empty indices / empty Other at the presenter boundary.
#[must_use]
pub fn compose_choose_none_callback(session_id: &str, question_index: u32) -> String {
    format!("{CHOOSE_NONE_CB_PREFIX}{session_id}:{question_index}")
}

/// Callback wire for Tap **Choose recommended** (metadata carried server-side until GREEN).
#[must_use]
pub fn compose_choose_recommended_callback(session_id: &str, question_index: u32) -> String {
    format!("{CHOOSE_RECOMMENDED_CB_PREFIX}{session_id}:{question_index}")
}

#[must_use]
pub fn callback_within_telegram_wire_limit(cb: &str) -> bool {
    cb.len() <= 64
}

/// Outbound shortcut rows appended to MultiSelect elicitation on the primary token only.
///
/// Labels must match acceptance tests (operator-facing copy). Omit **Choose recommended** when
/// `recommended_other_trimmed` is empty — no fabricated default.
#[must_use]
pub fn build_multi_select_shortcut_keyboard_rows(
    session_id: &str,
    question_index: u32,
    recommended_other_trimmed: &str,
) -> Option<InlineKeyboardRows> {
    log::debug!(
        target: "tddy_daemon::telegram_multi_select_shortcuts",
        "build_multi_select_shortcut_keyboard_rows: session_len={} question_index={} recommended_present={}",
        session_id.len(),
        question_index,
        !recommended_other_trimmed.trim().is_empty()
    );

    let none_cb = compose_choose_none_callback(session_id, question_index);
    if !callback_within_telegram_wire_limit(&none_cb) {
        log::warn!(
            target: "tddy_daemon::telegram_multi_select_shortcuts",
            "choose-none callback exceeds Telegram limit ({} bytes); skipping shortcuts",
            none_cb.len()
        );
        return None;
    }

    let mut row: Vec<(String, String)> = vec![("Choose none".to_string(), none_cb)];

    let rec_trim = recommended_other_trimmed.trim();
    if !rec_trim.is_empty() {
        let rec_cb = compose_choose_recommended_callback(session_id, question_index);
        if callback_within_telegram_wire_limit(&rec_cb) {
            row.push(("Choose recommended".to_string(), rec_cb));
            log::debug!(
                target: "tddy_daemon::telegram_multi_select_shortcuts",
                "including Choose recommended button (recommended_other len {})",
                rec_trim.len()
            );
        } else {
            log::warn!(
                target: "tddy_daemon::telegram_multi_select_shortcuts",
                "choose-recommended callback exceeds Telegram limit ({} bytes); omitting button",
                rec_cb.len()
            );
        }
    }

    log::info!(
        target: "tddy_daemon::telegram_multi_select_shortcuts",
        "built shortcut row_len={}",
        row.len()
    );

    Some(vec![row])
}

#[cfg(test)]
mod telegram_multi_select_shortcuts_red_tests {
    use super::*;

    #[test]
    fn build_shortcut_keyboard_yields_compact_rows_for_primary_elicitation() {
        let sid = "01900000-0000-7000-8000-0000000000aa";
        let rows = build_multi_select_shortcut_keyboard_rows(sid, 0, "fixture recommendation");
        assert!(
            rows.as_ref().is_some_and(|r| !r.is_empty()),
            "GREEN must attach shortcut rows containing Choose none (+ recommended when meta present); got {:?}",
            rows
        );
    }
}
