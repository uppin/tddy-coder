//! The bus's view of a session's pending question (`#home-inbox` 3/8).
//!
//! `tddy_changeset::pending_question::PendingQuestion` is the record on disk;
//! [`tddy_daemon_kernel::session_notification::QuestionNotice`] is what a notification carries,
//! because the kernel does not depend on `tddy-changeset`. Neither type is this crate's, so the
//! conversion is a function: a `From` impl between two foreign types is refused by the orphan rule.

use tddy_core::pending_question::PendingQuestion;
use tddy_daemon_kernel::session_notification::{
    QuestionNotice, QuestionNoticeItem, QuestionNoticeOption,
};

/// The notice that carries `pending` on a notification.
#[must_use]
pub fn question_notice_for(pending: &PendingQuestion) -> QuestionNotice {
    QuestionNotice {
        request_id: pending.request_id.clone(),
        kind: pending.kind.clone(),
        tool_name: pending.tool_name.clone(),
        summary: pending.summary(),
        questions: pending
            .questions
            .iter()
            .map(|item| QuestionNoticeItem {
                header: item.header.clone(),
                question: item.question.clone(),
                options: item
                    .options
                    .iter()
                    .map(|option| QuestionNoticeOption {
                        label: option.label.clone(),
                        description: option.description.clone(),
                    })
                    .collect(),
                multi_select: item.multi_select,
            })
            .collect(),
    }
}
