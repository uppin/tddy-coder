# oversized-file: svc_activity_ports.rs

**Location:** `packages/tddy-session-lifecycle/src/connection_service/svc_activity_ports.rs`
**Category:** oversized-file
**Detected:** 2026-11-16 by `/pr-wrap` step 3.5 file-length gate on PR #920 (`#remote-shell` 6/7), production lines counted to the first `#[cfg(test)]`
**Metrics:** **513 → 546 production lines** (2026-11-16) · budget 500
**Thresholds breached:** length 546 > 500
**Restructure:** `move_item` / `move_impl_members` into a submodule — `/code-restructuring` territory
**Status:** Open — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---:|---|
| 2026-11-16 | 513 → 546 | #920 (`#remote-shell` 6/7): `ActivityHookDelivery`, the in-process delivery of a target session's followed hooks. Already over the budget at the base; this PR grew it. **Not split**: successor PR #921 (`feature/remote-shell/coder-hosts`) also modifies this file, and a split here would turn its diff into a conflict (`/pr-wrap` step 3.5, stack-branch rule). Split after the `#remote-shell` stack lands |

## What would close it

Move `ActivityHookDelivery` (the `HookReportDelivery` impl) into its own submodule beside
`svc_activity_ports`. Anchor with `tddy-tools restructure anchors`, never by hand, prove the
seam with `restructure check --deep` against a warm index (`./run-index-daemon`), and confirm
the file and the new module are each ≤ 500 production lines. Do it on a follow-up branch after
the `#remote-shell` stack lands, when no stack PR still edits this file.

## Verified by hand

2026-11-16 (#920 wrap): counted with the step 3.5 gate loop — 513 at
`origin/feature/remote-shell/derived-hosts` (`a76247e46`), 546 at HEAD; confirmed #921 modifies
the same file in its own diff (`git diff origin/feature/remote-shell/remote-hooks...origin/feature/remote-shell/coder-hosts --name-only`).
