//! Kind-filtered subscribers of one repository.

use tokio::sync::mpsc;

use crate::{EventKind, InboxEvent, InboxSubscription, RowKey};

impl RowKey {
    /// The kind of change a subscriber hears when this row changes.
    pub fn event_kind(&self) -> EventKind {
        match self {
            RowKey::Pr { .. } | RowKey::PrForHead { .. } | RowKey::PrCommits { .. } => {
                EventKind::Pr
            }
            RowKey::ChecksAt { .. } => EventKind::Checks,
            RowKey::Conversation { .. } => EventKind::Comments,
            RowKey::BranchCommits { .. } => EventKind::Push,
        }
    }
}

struct Subscriber {
    kinds: Vec<EventKind>,
    sender: mpsc::UnboundedSender<InboxEvent>,
}

/// One repository's subscribers.
#[derive(Default)]
pub(crate) struct Subscribers {
    list: Vec<Subscriber>,
}

impl Subscribers {
    pub(crate) fn add(&mut self, kinds: &[EventKind]) -> InboxSubscription {
        let (sender, receiver) = mpsc::unbounded_channel();
        self.list.push(Subscriber {
            kinds: kinds.to_vec(),
            sender,
        });
        InboxSubscription { receiver }
    }

    /// Whether anyone is listening, without forgetting the ones that dropped.
    pub(crate) fn is_listening(&self) -> bool {
        self.list.iter().any(|s| !s.sender.is_closed())
    }

    /// Whether anyone is still listening; forgets the subscribers that dropped.
    pub(crate) fn any_listening(&mut self) -> bool {
        self.list.retain(|s| !s.sender.is_closed());
        !self.list.is_empty()
    }

    /// Tell every subscriber of `event.kind` about it.
    pub(crate) fn publish(&mut self, event: &InboxEvent) {
        self.list.retain(|s| {
            if !s.kinds.contains(&event.kind) {
                return !s.sender.is_closed();
            }
            s.sender.send(event.clone()).is_ok()
        });
    }
}
