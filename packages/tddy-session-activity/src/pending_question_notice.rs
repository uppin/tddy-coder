//! The bus's view of a session's pending question (`#home-inbox` 3/8).
//!
//! `tddy_changeset::pending_question::PendingQuestion` is the record on disk;
//! [`tddy_daemon_kernel::session_notification::QuestionNotice`] is what a notification carries,
//! because the kernel does not depend on `tddy-changeset`. Neither type is this crate's, so the
//! conversion is a function: a `From` impl between two foreign types is refused by the orphan rule.

use tddy_core::pending_question::PendingQuestion;
use tddy_daemon_kernel::session_notification::QuestionNotice;

/// The notice that carries `pending` on a notification.
#[must_use]
pub fn question_notice_for(pending: &PendingQuestion) -> QuestionNotice {
    // TODO(elicit-content): implement — request_id, kind, tool_name, `pending.summary()` and every
    // question with its options.
    let _ = pending;
    QuestionNotice::default()
}
