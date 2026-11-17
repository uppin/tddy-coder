# oversized-file: project_notification_log/storage.rs

**Location:** `packages/tddy-session-activity/src/project_notification_log/storage.rs`
**Category:** oversized-file
**Detected:** 2026-11-17 — `/pr-wrap` step 3.5 file-length gate on #930 (`#activity-wake` 2/3), production lines counted to the first `#[cfg(test)]`
**Metrics:** **536 → 578 production lines** (2026-11-17, #935) · budget 500
**Thresholds breached:** length 578 > 500
**Restructure:** `move_item` into a submodule — `/code-restructuring` territory
**Status:** Open — deferred to a follow-up branch after the `#activity-wake` stack lands; unclaimed

Not split in #930: predecessor PR #929 (`feature/activity-wake/session-ended-notice`) also modifies
this file, so the split is deferred under `/pr-wrap` step 3.5's stack-branch rule. #930 grew it by
the `activity_status` DTO field and its two conversions. Not split in #935 either: #929 and #930
are still open in the same stack, and #935 grew it by the `message` DTO (`MessageDto`) and its two
conversions.

## Measurement history

| Run | Production lines | Note |
|---|---:|---|
| 2026-11-17 | 530 → 536 | #930: the record DTO carries `activity_status` |
| 2026-11-17 | 536 → 578 | #935: the record DTO carries `message` (`MessageDto`, `SourceDto::Message`, the two `From` impls). Still stack-shared with the open #929/#930 — the split stays deferred to a follow-up after the stack lands |

## What would close it

Move the DTO conversions (`RecordDto` and the kind/source/level/agent/ended/question DTOs with
their `From` impls, about 330 lines) into `project_notification_log/storage/dto.rs` with
`tddy-tools restructure` intents proved by `restructure check --deep`, and confirm the file and
the new module are each ≤ 500 production lines.
