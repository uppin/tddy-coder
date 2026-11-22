//! The messages a project's window has evicted, kept so `ReadSessionMessages` can say how many of
//! the reader's own went before it read them: once a record leaves the window, the window can no
//! longer tell whose it was.

use std::collections::VecDeque;

use super::{Held, ProjectNotificationLog, ProjectState, PROJECT_NOTIFICATION_CAPACITY};

/// The evicted messages of one project, oldest first, at most [`PROJECT_NOTIFICATION_CAPACITY`].
#[derive(Default)]
pub(super) struct EvictedMessages(VecDeque<EvictedMessage>);

/// A message record the window evicted: its `seq`, and whose inbox it was addressed to.
struct EvictedMessage {
    seq: u64,
    os_user: String,
    to_session_id: String,
}

impl EvictedMessages {
    /// The records a restored log keeps in its window — all but the oldest `skip` — and the
    /// messages among those `skip` it evicted.
    pub(super) fn split_restored(records: Vec<Held>, skip: usize) -> (VecDeque<Held>, Self) {
        let mut evicted = Self::default();
        let mut records = records.into_iter();
        records
            .by_ref()
            .take(skip)
            .for_each(|held| evicted.note(&held));
        (records.collect(), evicted)
    }

    /// Note that `held` left the window, when it is a message.
    pub(super) fn note(&mut self, held: &Held) {
        let Some(message) = held.record.message.as_ref() else {
            return;
        };
        self.0.push_back(EvictedMessage {
            seq: held.record.seq,
            os_user: held.os_user.clone(),
            to_session_id: message.to_session_id.clone(),
        });
        if self.0.len() > PROJECT_NOTIFICATION_CAPACITY {
            self.0.pop_front();
        }
    }

    /// How many evicted messages after `seq` were addressed to `to_session_id` of `os_user`.
    pub(super) fn addressed_to(&self, os_user: &str, to_session_id: &str, seq: u64) -> u64 {
        self.0
            .iter()
            .filter(|evicted| {
                evicted.seq > seq
                    && evicted.os_user == os_user
                    && evicted.to_session_id == to_session_id
            })
            .count() as u64
    }
}

impl ProjectNotificationLog {
    /// How many messages addressed to `to_session_id` (of `os_user`, in `project_id`) after `seq`
    /// the log evicted before they could be read: `ReadSessionMessages`' `lost`. Other evicted
    /// records — activity, statuses, other sessions' messages — are not the reader's loss.
    pub fn messages_lost_after(
        &self,
        project_id: &str,
        os_user: &str,
        to_session_id: &str,
        seq: u64,
    ) -> u64 {
        let project = self.project(project_id);
        let lost = project
            .lock()
            .evicted_messages
            .addressed_to(os_user, to_session_id, seq);
        lost
    }
}

impl ProjectState {
    /// Append `held` to the window, evicting its oldest record once it holds more than
    /// [`PROJECT_NOTIFICATION_CAPACITY`] — and noting it when it is a message.
    pub(super) fn keep_in_window(&mut self, held: Held) {
        self.window.push_back(held);
        if self.window.len() > PROJECT_NOTIFICATION_CAPACITY {
            if let Some(evicted) = self.window.pop_front() {
                self.evicted_messages.note(&evicted);
            }
        }
    }
}
