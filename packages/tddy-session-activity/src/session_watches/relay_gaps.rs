//! Relay gaps in a watch's delivery (n5, cross-host watches): records another host evicted before
//! it could relay them here, reported to every watch as lost — once each, and still whole after this
//! host's own window has evicted the gap record.
//!
//! Feature: docs/ft/daemon/project-concept.md#watching-sessions-on-other-hosts

use std::collections::BTreeMap;

use crate::project_notification_log::{ProjectNotificationLog, ProjectNotificationRecord};
use crate::session_watches::SessionWatch;

/// The relay gaps among `records` that `watch` has not reported yet: each gap's `seq` and how many
/// records it lost. A gap is never an event, so it never moves a position; it is marked reported
/// in [`SessionWatch::acked_above`] instead ([`note_gaps_reported`]), and pruned from there as the
/// position passes it.
pub(crate) fn unreported_gaps<'a>(
    watch: &'a SessionWatch,
    records: &'a [ProjectNotificationRecord],
) -> impl Iterator<Item = (u64, u64)> + 'a {
    records
        .iter()
        .filter(move |record| {
            record.relay_lost > 0
                && record.seq > watch.position
                && !watch.acked_above.contains(&record.seq)
        })
        .map(|record| (record.seq, record.relay_lost))
}

/// Mark `gaps` reported on `watch`, past its position.
pub(crate) fn note_gaps_reported(watch: &mut SessionWatch, gaps: &BTreeMap<u64, u64>) {
    for seq in gaps.keys().filter(|seq| **seq > watch.position) {
        watch.acked_above.insert(*seq);
    }
}

/// What the relay gaps the log evicted after `watch`'s position stood for beyond the one record
/// each, when the watch's scan found evictions (`watch_lost`) at all.
pub(crate) fn evicted_gaps_lost(
    log: &ProjectNotificationLog,
    watch: &SessionWatch,
    watch_lost: u64,
) -> u64 {
    if watch_lost == 0 {
        return 0;
    }
    log.evicted_relay_lost_after(
        &watch.owner.project_id,
        &watch.owner.os_user,
        watch.position,
    )
}

/// Mark `gaps` reported on each of `watches`, and how many records they stand for in all.
pub(crate) fn report_gaps<'a>(
    watches: impl Iterator<Item = &'a mut SessionWatch>,
    gaps: &BTreeMap<u64, u64>,
) -> u64 {
    if gaps.is_empty() {
        return 0;
    }
    for watch in watches {
        note_gaps_reported(watch, gaps);
    }
    gaps.values().sum()
}
