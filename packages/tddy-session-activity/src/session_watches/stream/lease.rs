//! The leases a stream holds on the watches it streams (`#activity-wake` 4/5), in memory only.

use std::collections::{BTreeSet, HashMap};

use crate::session_watches::WatchOwner;

/// One watch's lease: the stream holding it, when the lease lapses, the newest `seq` it was
/// handed, and the `seq`s of this watch's records in the stream's latest answer — what its next
/// `ack_seq` may acknowledge.
#[derive(Debug, Clone)]
pub(in crate::session_watches) struct Lease {
    pub(in crate::session_watches) owner: WatchOwner,
    pub(in crate::session_watches) stream_id: String,
    pub(in crate::session_watches) lease_until_unix_ms: u64,
    pub(in crate::session_watches) handed: u64,
    pub(in crate::session_watches) latest_batch: BTreeSet<u64>,
}

/// The in-memory leases of every stream on this host.
#[derive(Debug, Default)]
pub(in crate::session_watches) struct StreamLeases {
    pub(in crate::session_watches) by_watch: HashMap<String, Lease>,
}
