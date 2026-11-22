# Changeset: A session's inspector shows its PR and its worktree's sync state

**Date**: 2026-11-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — proto change (two `SessionEntry` fields, a streaming RPC, three worktree RPCs), several crates and two web packages, a stream fed by n2's subscriptions, git mutations (pull, push) on a live worktree
**Stack**: `#workqueue` 4/14 — branch `feature/workqueue/session-pr-tabs`, base `feature/workqueue/workitem-rename`

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

## Draft PR contract

The first push of this PR, before its implementation: the proto additions, the daemon handler
signatures, `worktree_sync`'s public functions, `defaultInspectorTab`, the empty web components with
their test ids, and the failing tests below.

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

`packages/tddy-worktree-service/tests/worktree_sync_acceptance.rs`:
- [ ] `a_clean_branch_level_with_its_upstream_is_in_sync`
- [ ] `local_commits_not_on_the_upstream_are_unpushed_and_listed`
- [ ] `upstream_commits_not_local_are_unpulled_and_listed`
- [ ] `both_sides_ahead_is_diverged_and_offers_neither_pull_nor_push`
- [ ] `a_branch_with_no_upstream_is_unpublished_and_offers_push`
- [ ] `dirty_files_include_modified_staged_and_untracked_paths`
- [ ] `pull_fast_forwards_and_refuses_when_not_fast_forwardable`

`packages/tddy-daemon-rpc/tests/session_pull_request_stream_acceptance.rs`:
- [ ] `a_session_whose_branch_has_a_pr_lists_its_number_and_state`
- [ ] `the_stream_sends_the_pr_its_checks_comments_and_commits`
- [ ] `a_completed_check_reaches_the_open_stream`
- [ ] `ten_open_streams_of_one_pr_share_the_inbox_reads`

`packages/tddy-session-web/cypress/component/SessionInspectorPullRequestTabAcceptance.cy.tsx`:
- [ ] `a session with a PR opens its inspector on the PR tab`
- [ ] `a session without a PR opens its inspector on the Worktree tab`
- [ ] `an inspector param naming details still opens Details`
- [ ] `checks show running with elapsed time and completed with conclusion`
- [ ] `review comments are threaded under their first comment`

`packages/tddy-web/cypress/component/SessionWorktreeTabAcceptance.cy.tsx` (existing, extended):
- [ ] `lists dirty files and names the sync state`
- [ ] `offers Pull only when unpulled and Push only when unpushed`

## Decisions & Trade-offs

- **PR presence travels on `SessionEntry`** so the default tab is decided without a second round trip
  or a tab that jumps after load; the listing reads only the inbox cache.
- **The PR view is a server stream fed by inbox events**, not a browser poll.
- **Default tab (operator amendment, 2026-11-22):** Work queue when the session owns one (n5), else PR,
  else Worktree. Nothing here or in n5 checks a role.
- **No Commit action** — committing in a live agent worktree from the UI races the agent.
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
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
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
