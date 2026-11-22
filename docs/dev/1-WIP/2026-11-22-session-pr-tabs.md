# Changeset: A session's inspector shows its PR and its worktree's sync state

**Date**: 2026-11-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — proto change (two `SessionEntry` fields, a streaming RPC, three worktree RPCs), several crates and two web packages, a stream fed by n2's subscriptions, git mutations (pull, push) on a live worktree
**Stack**: `#workqueue` 4/14 — branch `feature/workqueue/session-pr-tabs`, base `feature/workqueue/workitem-rename`
**PR**: [#1005](https://github.com/uppin/tddy-coder/pull/1005)

## Initial Discovery

[2026-11-22-session-pr-tabs-initial-discovery.md](./2026-11-22-session-pr-tabs-initial-discovery.md)

## Affected Packages

- **tddy-service**: `session.proto` `SessionEntry.{pr_number, pr_state}`; new `session_pull_request.proto` (`SessionPullRequestService.StreamSessionPullRequest`); `worktree.proto` `GetWorktreeSyncState`, `PullWorktree`, `PushWorktree`; `tests/unbundle_service_split.rs` closed lists
- **tddy-worktree-service**: `worktree_sync.rs` — dirty files (`status --porcelain -z --untracked-files=all`), upstream relation by ancestry both ways, unpushed/unpulled commit lists, ff-only pull, plain push
- **tddy-daemon-rpc**: the PR-view stream over the inbox (subscribe `{Pr, Checks, Comments}` for the session's repo, filter by its branch/PR); worktree RPC handlers; peer forwarding like the existing worktree RPCs
- **tddy-session-activity**: listing enrichment fills `pr_number`/`pr_state` from the inbox's cached `pr_for_head` row (never a blocking GitHub call)
- **tddy-workspace-web**: `SessionWorktreeTab` new sections + `useWorktreeSyncState`
- **tddy-session-web**: `SessionPullRequestTab`, `CheckRow`, comment thread view; `InspectorTabs.tsx` PR button; `SessionInspectorDrawer.tsx` dispatch and `defaultInspectorTab`
- **tddy-client-web**: `INSPECTOR_TAB_NAMES` gains `pr`; regenerated TS

## Related Feature Documentation

- PRD: [PRD-2026-11-22-session-pr-tabs.md](../../ft/web/1-WIP/PRD-2026-11-22-session-pr-tabs.md)
- Wrap updates: `docs/ft/web/session-worktree-inspector.md` (also corrects its drift: it still describes `ListWorktreesForProject` + a 10-minute refresh), `session-drawer.md`, `url-state-routing.md`; new `docs/ft/web/session-pr-inspector.md`

## Summary

A PR tab (status, checks, threaded comments, commits — live from n2's inbox), Worktree-tab sync state
with Pull/Push, and the default-tab rule PR → Worktree.

## Background

See the PRD and Exploration 2 §4B: the Worktree tab exists (`tddy-workspace-web/src/worktrees/SessionWorktreeTab.tsx`),
no PR tab exists, no RPC returns upstream ahead/behind or a dirty list for a session, and the session
list carries no PR.

## Responsibility

- `SessionEntry.pr_number`, `SessionEntry.pr_state` and their enrichment from the inbox cache.
- `StreamSessionPullRequest` and its daemon handler.
- `GetWorktreeSyncState`, `PullWorktree`, `PushWorktree` and `tddy-worktree-service::worktree_sync`.
- `SessionPullRequestTab`, `CheckRow`, `CommentThread`, `CommitList` (web) — the PR view later nodes
  reuse for work items.
- `defaultInspectorTab(session)` — PR when `pr_number` is set, else Worktree; an explicit
  `?inspector=` wins. Owned here; n5 adds a precedence to it.

## Boundaries

- No Inbox/Outbox tabs in the PR view (n11).
- No work-queue tab or precedence (n5).
- No Commit action (the agent owns committing in its worktree); no force push.
- No GitHub mutation from the PR tab (no merge, no re-run of checks).
- Target sessions: no target-side sync-state reader here; the section reports it unavailable and a
  TODO records the gap.
- Does not change the inbox (n2).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n2` github-event-inbox (#1003) | `GithubInbox::{subscribe, pr_for_head, checks_at, comments, pr_commits}`, `EventKind`, richer `CheckRun` | the PR stream subscribes and reads rows; listing reads cached `pr_for_head` | call GitHub directly, add a cache, change row keys or the check-run shape |
| `n3` workitem-rename (#1004) | renamed vocabulary (line predecessor only) | — | — |

**Sequencing (pass 2):** `pull_request_view_acceptance.rs` keeps n2's real inbox — reading through it is
the point — so those five tests pass only once n2 is green (`a_cached_branch_summarises…` reaches n2's
`pr_for_head` in its Given). `worktree_sync_acceptance.rs` and both Cypress specs need nothing of n2.

## Draft PR contract

Published (pass 2, commit 2):

- **Proto**: `SessionEntry.pr_number` (42), `pr_state` (43); on `WorktreeService`:
  `GetWorktreeSyncState`, `PullWorktree`, `PushWorktree` (`SessionWorktreeRequest` → `WorktreeSyncState`)
  and `StreamSessionPullRequest` (→ stream `SessionPullRequestView`, with `PullRequestCheck`,
  `PullRequestComment`, `PullRequestCommit`). TS regenerated in both copies.
- **`tddy-worktree-service`**: `worktree_sync::{SyncState, SyncStatus, CommitLine, DirtyFile, DirtyChange,
  WorktreeSyncError, read_sync_state, pull_fast_forward, push_branch}` and
  `pull_request_view::{PullRequestSummary, pull_request_summary, pull_request_view, watch_pull_request,
  PullRequestWatch}` (`todo!("TODO(n4): …")`); `WorktreeServiceImpl` answers the four RPCs
  `unimplemented` until `/green`.
- **Web**: `tddy-workspace-web` exports `SessionPullRequestTab`, `CheckRow`, `CommitList` (shells);
  `tddy-session-web/src/defaultInspectorTab.ts` (`defaultInspectorTab`, throws until `/green` — not yet
  wired into the drawer).
- The failing tests below.

`/green` must also update two existing assertions this node changes on purpose:
`SessionInspectorAcceptance` "Details is selected by default" and `SessionInspectorUrlStateAcceptance`
"choosing an inspector tab… landing on the Details tab".

## Green wave

**Wave:** 2 of 8
**Greenable independently:** partly — the Worktree half needs nothing; the PR stream and listing tests exercise n2's inbox behaviour (a scripted GitHub behind a real inbox), so the PR half waits for n2 green
**Concurrent with:** n3 workitem-rename
**Blocks:** n11 commit-inbox-outbox (adds Inbox/Outbox to this PR view, reusing `CommitList`)

Real dependency edges (the line is n1 · n2 · n3 · … · n14; `a → b` means b needs a's behaviour):

    n1 remove-pr-stack-recipe → n3 workitem-rename
    n2 github-event-inbox     → n4 session-pr-tabs, n6 pr-and-session-link
    n3 workitem-rename        → n5 session-work-queue
    n4 session-pr-tabs        → n11 commit-inbox-outbox
    n5 session-work-queue     → n6 pr-and-session-link, n7 badges-labels, n8 move-activities
    n6 pr-and-session-link    → n9 lineage-graph
    n9 lineage-graph          → n10 waves-line, n11 commit-inbox-outbox, n12 leak-linearity
    n10 waves-line            → n13 schedules
    n11 commit-inbox-outbox   → n13 schedules
    n13 schedules             → n14 land-propagate

Waves: 1 = n1, n2 · 2 = n3, n4 · 3 = n5 · 4 = n6, n7, n8 · 5 = n9 · 6 = n10, n11, n12 · 7 = n13 ·
8 = n14 (run along the line `1 1 2 2 3 4 4 4 5 6 6 6 7 8` — never steps back). Inside each wave the
blockers lead: n1 (12 transitive dependents) before n2 (8); n3 (10) before n4 (3); n6 (6) before n7
and n8 (0 each); n10 and n11 (2 each — tie, kept in plan order) before n12 (0). The sweep node is
reserved and planned after the pipelines drain.

## Prerequisites

### ⚠ DURING — [2026-10-23-session-status-streams-do-not-reconnect.md](../todo/2026-10-23-session-status-streams-do-not-reconnect.md)

The PR stream is a new browser stream: it must re-subscribe on error with backoff (the entry's
proposed shared helper if it exists by then, else the tab's own), never sit on an error.

### ⚠ DURING — [2026-11-07-session-web-specs-do-not-typecheck.md](../todo/2026-11-07-session-web-specs-do-not-typecheck.md)

New specs must typecheck; the count must not grow.

### ⚠ DURING — [oversized-file-session-main-pane.md](../../../packages/tddy-session-web/docs/code-issues/oversized-file-session-main-pane.md)

The inspector is mounted from `SessionMainPane.tsx` (504 lines): the new client prop is threaded
without growing it — through the existing clients bundle.

### Related — [2026-08-01-tddy-web-inactive-session-activities-follow-ups.md](../todo/2026-08-01-tddy-web-inactive-session-activities-follow-ups.md)

Same drawer; untouched.

## Scope

- [ ] `SessionEntry` PR fields from the inbox cache
- [ ] `StreamSessionPullRequest` over the inbox
- [ ] Worktree sync state + Pull/Push RPCs and git implementation
- [ ] PR tab (status, checks, comments, commits)
- [ ] Worktree tab sections and actions
- [ ] Default tab PR → Worktree
- [ ] Docs updated through the wrap

## Technical Changes

### State A (Current)

- `SessionInspectorDrawer.tsx:137-138`: `requestedTab = isInspectorTabName(tabParam) ? tabParam : "details"`.
- `InspectorTabs.tsx`: nine hand-written buttons; `appRoutes.ts:239-286` `INSPECTOR_TAB_NAMES`.
- `SessionWorktreeTab.tsx`: disk, last calculated, branch, changed count; Refresh/Clear/Delete/Restore.
- `worktree.proto` / `session.proto`: no upstream relation, no PR.
- `base_freshness.rs:121` has the only `@{upstream}` count (`HEAD..@{upstream}`).

### State B (Target)

```
Inspector ─┬─ PR tab ── StreamSessionPullRequest(session) ── daemon ── GithubInbox.subscribe(repo,{Pr,Checks,Comments})
           │                                                         └ reads pr_for_head(branch), checks_at(head), comments(#n), pr_commits(#n)
           └─ Worktree tab ── GetWorktreeSyncState / PullWorktree / PushWorktree ── worktree_sync (git in the session's worktree)
Default tab: ?inspector= → else PR if SessionEntry.pr_number → else Worktree
```

Sync state: `in_sync | unpushed | unpulled | diverged | unpublished` decided by `merge-base
--is-ancestor` both ways (a merge commit counts); Pull offered when unpulled (ff-only), Push when
unpushed or unpublished, neither while diverged.

### Delta

As listed under Affected Packages. New RPCs are registered in `unbundle_service_split.rs`'s closed lists.

## Implementation Milestones

- [ ] `worktree_sync` git reads + pull/push, temp-repo tests
- [ ] Worktree RPCs + handlers + peer forwarding
- [ ] `SessionEntry` PR enrichment from the inbox cache
- [ ] PR stream handler over inbox subscriptions
- [ ] Web: PR tab, Worktree sections, default tab

## Testing Plan

- Rust integration: `worktree_sync` against temp repos with a bare origin (scripted ahead/behind/
  diverged/unpublished); daemon handlers over the in-memory RPC path (no LiveKit) with an inbox over a
  scripted GitHub (n2's doubles).
- Web: Cypress component specs mounting the inspector with fake clients; the stream is faked at the
  client port.

## Acceptance Tests

`packages/tddy-worktree-service/tests/worktree_sync_acceptance.rs` (real repos: a bare origin, the session clone, a second clone):
- [x] `a_clean_branch_level_with_its_upstream_is_in_sync`
- [x] `local_commits_not_on_the_upstream_are_unpushed_and_listed`
- [x] `upstream_commits_not_local_are_unpulled_and_listed`
- [x] `both_sides_ahead_is_diverged_and_offers_neither_pull_nor_push`
- [x] `a_branch_with_no_upstream_is_unpublished_and_offers_push`
- [x] `dirty_files_include_modified_staged_and_untracked_paths`
- [x] `pull_fast_forwards_and_refuses_when_not_fast_forwardable`
- [x] `push_publishes_an_unpublished_branch_and_leaves_it_in_sync`

`packages/tddy-worktree-service/tests/pull_request_view_acceptance.rs` (an inbox over a scripted GitHub —
the handler lives in `tddy-worktree-service`, so these stay out of `tddy-daemon-rpc`'s test build):
- [x] `the_view_carries_the_pr_its_checks_conversation_and_commits`
- [x] `a_branch_without_a_pr_is_a_view_that_does_not_exist`
- [x] `a_cold_branch_has_no_summary_and_makes_no_request` — what listing calls; it never waits on GitHub
- [x] `a_cached_branch_summarises_its_pr_number_and_state`
- [x] `a_completed_check_reaches_an_open_watch`

"Ten open streams share the inbox reads" is n2's `repeated_reads_of_an_unchanged_pr_for_head_row_make_no_further_requests`.

`packages/tddy-session-web/cypress/component/SessionInspectorPullRequestTabAcceptance.cy.tsx` (in-memory backend):
- [x] `a session with a PR opens its inspector on the PR tab`
- [x] `a session without a PR opens its inspector on the Worktree tab`
- [x] `an inspector param naming details still opens Details`
- [x] `checks show running with elapsed time and completed with conclusion`
- [x] `review comments are threaded under their first comment`

`packages/tddy-workspace-web/cypress/component/SessionWorktreeTabSyncStateAcceptance.cy.tsx` (the component's own package):
- [x] `lists dirty files and names the sync state`
- [x] `offers Pull only when unpulled and Push only when unpushed`

## Decisions & Trade-offs

- **PR presence travels on `SessionEntry`** so the default tab is decided without a second round trip
  or a tab that jumps after load; the listing reads only the inbox cache.
- **The PR view is a server stream fed by inbox events**, not a browser poll.
- **Default tab (operator amendment, 2026-11-22):** Work queue when the session owns one (n5), else PR,
  else Worktree. Nothing here or in n5 checks a role.
- **No Commit action** — committing in a live agent worktree from the UI races the agent.
- **RPCs on `WorktreeService`, not a new proto file** (pass 2): all four serve the session inspector and
  `WorktreeService` already addresses sessions; a new file meant codegen registration in four places.
- **PR view components in `tddy-workspace-web`** (pass 2), beside `SessionWorktreeTab`: n6's work-item
  rows reuse `CheckRow`, and the queue package cannot depend on `tddy-session-web` (it is a dependency
  of it).
- Splitting this node into a PR-tab node and a Worktree-tab node remains possible if review size
  demands it; they share the default-tab rule, which is why they were planned together.

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-22-session-pr-tabs-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
