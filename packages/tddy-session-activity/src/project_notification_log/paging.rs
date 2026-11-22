use crate::project_notification_log::Held;
use crate::project_notification_log::ProjectNotificationRecord;
use crate::project_notification_log::ProjectNotificationScope;
use crate::project_notification_log::ProjectState;
use crate::project_notification_log::Start;
use tddy_tool_engine::session_follow::FollowPage;

/// What `state` holds for `scope` from `start`: a page of at most `size` records, and the number
/// of records the start lost to eviction.
///
/// The page's `next_cursor` is its last record's `seq`, or, for an empty page, the log's newest
/// `seq`. `has_more` says whether a record of `scope` follows the page's end.
pub(super) fn page_of(
    state: &ProjectState,
    scope: &ProjectNotificationScope,
    start: Start,
    (size, clamped_to): (usize, Option<usize>),
) -> (FollowPage<ProjectNotificationRecord>, u64) {
    let visible = state.window.iter().filter(|held| held.visible_to(scope));
    let (held, has_more, lost) = match start {
        Start::Newest => {
            let all: Vec<&Held> = visible.collect();
            let first = all.len().saturating_sub(size);
            (all[first..].to_vec(), false, 0)
        }
        Start::After(seq) => {
            let after: Vec<&Held> = visible.filter(|held| held.record.seq > seq).collect();
            let has_more = after.len() > size;
            (
                after.into_iter().take(size).collect(),
                has_more,
                lost_after(state, seq),
            )
        }
    };
    let next_cursor = held
        .last()
        .map_or(state.newest_seq, |held| held.record.seq)
        .to_string();
    let items = held.into_iter().map(|held| held.record.clone()).collect();
    (
        FollowPage {
            items,
            next_cursor,
            has_more,
            clamped_to,
        },
        lost,
    )
}

/// How many records between `seq` and the window's oldest were evicted before they were read.
pub(super) fn lost_after(state: &ProjectState, seq: u64) -> u64 {
    let Some(oldest) = state.window.front() else {
        return 0;
    };
    // The records numbered after `seq` and before the oldest kept one are the evicted ones.
    let last_evicted = oldest.record.seq.saturating_sub(1);
    last_evicted.saturating_sub(seq)
}
