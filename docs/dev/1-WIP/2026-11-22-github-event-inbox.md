# Changeset: GitHub event inbox

**Date**: 2026-11-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — new crate plus three re-wired consumers across crates, concurrency (single-flight, shared per-repo task), an HTTP adapter seam, cache invalidation semantics
**Stack**: `#workqueue` 2/14 — branch `feature/workqueue/github-event-inbox`, base `feature/workqueue/remove-pr-stack-recipe`
**PR**: [#1003](https://github.com/uppin/tddy-coder/pull/1003)
**Plan phase**: two commits (plan, then the operator's trunk-watch decision) — pass-2 rebases expect two plan commits above the parent, and the contract is the third

## Initial Discovery

[2026-11-22-github-event-inbox-initial-discovery.md](./2026-11-22-github-event-inbox-initial-discovery.md)

## Affected Packages

- **tddy-github-inbox** (new): `GithubInbox`, the per-repo feed, the row cache, invalidations, single-flight, subscriptions
- **tddy-github**: conditional transport (`ConditionalRead` — status, ETag, poll interval, rate-limit headers) over REST and over `gh api -i`; `CheckRun` gains `id`, `status`, `started_at`, `completed_at`, `details_url`; repository events and updated-PR sweep reads
- **tddy-session-activity**: `trunk_watch` takes commits from the inbox
- **tddy-session-lifecycle**: `svc_trunk_watch_wiring.rs` builds the inbox's credential source instead of a per-project feed
- **tddy-daemon**: runtime mounts the inbox (`runtime/mount.rs`)
- ~~**tddy-daemon-rpc**: `pr_stack/pr_status.rs` (QueryBranch / GetPrStatus PR leg) and `exec_tool/stack_tools/ops.rs` (`status`) read through the inbox~~ — moved to n6 (#1007) by the operator's decision: it needs repo→project credential plumbing into `PrStackRpcHandler` (the inbox reads with a project's registered credential, `pr_status_for_caller` has a login-keyed token and only `repo_root`) and a sync→async adapter for `GithubPrApi` in the `ports.rs` area

## Related Feature Documentation

- PRD: [PRD-2026-11-22-github-event-inbox.md](../../ft/daemon/1-WIP/PRD-2026-11-22-github-event-inbox.md)
- Wrap updates: `docs/ft/daemon/project-concept.md` § Watching the trunk, `docs/ft/coder/pr-stack-live-status.md` § polling, `packages/tddy-github/docs/trunk-feed.md`, new `packages/tddy-github-inbox/docs/architecture.md`

## Summary

One conditional activity feed per repository, a row cache with per-row ETags, invalidations,
single-flight and kind-filtered subscriptions — the daemon's single path for reading GitHub PR state.
The trunk watch and the stack's live branch status move onto it.

## Background

See the PRD. Measured today (Exploration 2 §4A): one conditional reader (trunk feed); unconditional
curl everywhere else, response headers discarded; the stack view's 5 s `QueryBranch` poll costs one
`GET pulls?head=` per branch per view.

## Responsibility

- The `GithubInbox` service and its public read and subscribe API (below).
- The conditional transport in `tddy-github` and the richer `CheckRun`.
- Rate-limit aware pacing of the per-repo feed.
- Migrating the trunk watch onto it. (`QueryBranch`/`GetPrStatus`'s PR leg and the stack `status` tool: moved to n6 (#1007) by the operator's decision: it needs repo→project credential plumbing into `PrStackRpcHandler` (the inbox reads with a project's registered credential, `pr_status_for_caller` has a login-keyed token and only `repo_root`) and a sync→async adapter for `GithubPrApi` in the `ports.rs` area.)

Owned surface (names final in the contract commit):

| Symbol | Kind |
|---|---|
| `GithubInbox::{new, subscribe, step, pr, pr_for_head, peek_pr_for_head, checks_at, conversation, pr_commits, branch_commits}`, `LOW_QUOTA_REMAINING` | read + subscribe API (`step(now)` drives the feeds) |
| `InboxSubscription`, `InboxEvent { repo, kind, row }`, `EventKind { Pr, Checks, Comments, Push }`, `StepReport` | subscription types |
| `RowKey`, `Cached<T> { value, fetched_at_unix_ms, etag }`, `PrConversation { reviews, review_comments, issue_comments }` (reviews carry their state — n13's review condition reads it), `PrCommit { sha, author_name, author_date, subject }` (n12's leak check reads it), `InboxError`, `paths::*` | cache types |
| `RepoReaders` port (vault account or local gh, resolved per read); `InboxTrunkFeed` (a `TrunkFeed` over the branch-commits row) | ports |
| `tddy_github::conditional::{ConditionalRead, ConditionalAnswer, RateLimit, GithubReadError, RestConditionalRead, GhCliConditionalRead, GITHUB_API_BASE}`, `tddy_github::check_runs::parse_check_runs` | transport |
| `tddy_github::pr_api::CheckRun` new fields (`id`, `status: CheckStatus`, `started_at`, `completed_at`, `details_url`; `Default`) | data |

## Boundaries

- No session-facing watch kind for GitHub events (a later node adds the activities it needs).
- No change to who subscribes to the trunk, or to trunk-watch activity text.
- Agent PR reads and every PR mutation (merge, patch base, create) stay direct.
- No PR tab, no UI (n4); no work-item check badges (n6).
- No persistence across daemon restarts: the cache is in memory; the first read after a restart is unconditional.
- github.com only.

## Dependencies

None in behaviour — wave 1. It sits above n1 in the line only because the stack is linear; nothing it
does reads or changes what n1 removes.

## Draft PR contract

Published (pass 2, commit 3 — the plan phase is two commits): the `tddy-github-inbox` crate with
`GithubInbox`'s public signatures (`todo!("TODO(n2): …")` bodies), the subscription and cache types,
`InboxTrunkFeed`, `paths`; the `conditional` transport, `check_runs::parse_check_runs` and the richer
`CheckRun` in `tddy-github` (its three struct-literal construction sites take `..Default::default()`);
and the 14 failing tests below. `/green` implements the feed, cache and migrations in this PR.

Children consume: n4 (`subscribe`, `pr_for_head`, `peek_pr_for_head`, `checks_at`, `conversation`,
`pr_commits`, `CheckRun`), n6 (`subscribe`, `pr`, `checks_at`), n9 (`pr`), n12 (`pr_commits`, `pr`),
n13 (`checks_at`, `conversation`).

## Green wave

**Wave:** 1 of 8
**Greenable independently:** yes — the inbox is tested against a scripted GitHub; its consumers are re-wired here
**Concurrent with:** n1 remove-pr-stack-recipe
**Blocks:** n4 session-pr-tabs (the PR tab's status, checks, comments and commits are inbox reads); n6 pr-and-session-link (live checks on work items)

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

### ⚠ DURING — [2026-07-26-pr-stack-status-polling-and-stack-hygiene.md](../todo/2026-07-26-pr-stack-status-polling-and-stack-hygiene.md)

Its first item — "batch per stack, cache per branch with ETags, or back off when unchanged" — is
answered by routing the PR leg through the inbox. The entry's other items (dangling `session_id`,
fetch freshness, repoint silent failures) remain; it is narrowed at wrap, not deleted.

### ⚠ DURING — [2026-11-21-trunk-watch-reads-github-com-only.md](../todo/2026-11-21-trunk-watch-reads-github-com-only.md)

The inbox inherits the github.com-only repo parsing; the API base is kept a single constructor
argument so the enterprise fix stays one change.

### ⚠ DURING — [2026-07-30-pr-stack-full-control-follow-ups.md](../todo/2026-07-30-pr-stack-full-control-follow-ups.md)

"No testable transport seam for `RealGithubPrApi`": the inbox's transport is the seam for reads; the
curl-based mutations keep the gap.

### ⚠ DURING — [oversized-file-pr-api.md](../../../packages/tddy-github/docs/code-issues/oversized-file-pr-api.md)

`pr_api.rs` must not grow: the conditional transport and the new check-run fields' parsing go in new
modules.

### ⚠ DURING — [oversized-file-pr-stack-ports.md](../../../packages/tddy-daemon-rpc/docs/code-issues/oversized-file-pr-stack-ports.md)

The PR-leg change lives in `pr_status.rs`; `ports.rs` (880 lines) must not grow.

### Related — [2026-10-23-session-status-streams-do-not-reconnect.md](../todo/2026-10-23-session-status-streams-do-not-reconnect.md)

No new browser stream here; recorded for n4, which adds one.

## Scope

- [ ] `tddy-github-inbox` crate: feed task per repo, row cache with ETags, invalidations, single-flight, kind-filtered subscriptions, rate-limit pacing
- [ ] Conditional transport over REST and `gh api -i`; richer `CheckRun`
- [x] Trunk watch on the inbox's branch-commits row
- [ ] ~~`QueryBranch`/`GetPrStatus` PR leg and stack `status` through the inbox~~ — moved to n6 (#1007) by the operator's decision: it needs repo→project credential plumbing into `PrStackRpcHandler` (the inbox reads with a project's registered credential, `pr_status_for_caller` has a login-keyed token and only `repo_root`) and a sync→async adapter for `GithubPrApi` in the `ports.rs` area
- [ ] Docs updated through the wrap

## Technical Changes

### State A (Current)

- `tddy-github/src/trunk_feed.rs`: `TrunkFeed` (REST and `gh`) with one ETag per project in
  `TrunkPoller`; `X-Poll-Interval` honoured; 15-minute unconditional sweep.
- `tddy-github/src/github_rest_common.rs`: curl to a temp file, status code only, no headers.
- `tddy-daemon-rpc/src/pr_stack/pr_status.rs:17-75`: `pr_status_for_caller` → `get_pr_by_head`
  per call, login-keyed token.
- `tddy-prstack-web/src/useQueryBranch.ts:14`: 5 s poll per branch per view.
- `CheckRun { name, conclusion }`.

### State B (Target)

```
GitHub ──conditional reads──▶ GithubInbox(repo)
                               ├─ feed task: events (ETag, X-Poll-Interval) + sweep + pending-check poll
                               ├─ rows: pr#n · pr_for_head(b) · checks@sha · comments#n · commits#n@head · branch_commits(b)
                               │        each { value, etag, fetched_at }, single-flight on miss/stale
                               └─ subscribers(repo, {kinds}) ◀── InboxEvent on row change
consumers: trunk watch (branch_commits), QueryBranch/GetPrStatus + stack status (pr_for_head)
```

Event → invalidation: `PullRequestEvent` → `pr#n`, `pr_for_head(head)`, `commits#n`;
`PushEvent(ref)` → `branch_commits(ref)`, `pr_for_head(ref)`; `IssueCommentEvent`,
`PullRequestReviewEvent`, `PullRequestReviewCommentEvent` → `comments#n`; check runs → `checks@sha`
via the pending-check poll (the events feed does not carry them).

### Delta

- New crate `packages/tddy-github-inbox` (`feed.rs`, `rows.rs`, `in_flight.rs`, `subscriptions.rs`, `pacing.rs`).
- `tddy-github`: `conditional.rs` (REST + `gh`), `repo_events.rs`, `check_runs.rs`.
- `trunk_watch.rs`: `TrunkPoller` reads `branch_commits` through the inbox; `TrunkSource` builds from
  the inbox handle.
- `pr_status.rs`, `stack_tools/ops.rs`: PR lookups through the inbox.

## Implementation Milestones

- [x] Conditional transport + rate-limit headers, both credential paths
- [x] Row cache with ETag revalidation and single-flight
- [x] Feed task: events, sweep, pending-check poll, pacing
- [x] Subscriptions by repo and kind
- [x] Trunk watch migrated (its six acceptance tests unchanged and green)
- [ ] ~~PR leg and stack status migrated~~ — moved to n6 (#1007)

## Testing Plan

Integration tests against a local axum GitHub (the crate's existing stand-in) for the transport and the feed's HTTP
behaviour; port doubles (a scripted `ConditionalRead`) for cache, single-flight and subscription
semantics. Time is injected (the trunk poller's step-function pattern), never slept. Consumer
migrations are proved by their existing suites (`trunk_watch_acceptance.rs` 6,
`tddy-github/tests/trunk_feed.rs`, `project_stacks_rpc_acceptance.rs`) plus one request-count test.

## Acceptance Tests

`packages/tddy-github-inbox/tests/inbox_feed_acceptance.rs`:
- [x] `two_subscribers_of_one_repository_share_one_feed`
- [x] `a_not_modified_feed_answer_changes_nothing_and_waits_out_the_poll_interval`
- [x] `a_pull_request_event_invalidates_only_that_pull_requests_rows`
- [x] `a_stale_row_is_revalidated_with_its_stored_etag`
- [x] `concurrent_reads_of_one_row_make_one_request`
- [x] `a_checks_subscriber_hears_a_pending_check_complete_and_not_a_new_comment`
- [x] `checks_are_polled_only_while_some_head_has_pending_checks`
- [x] `a_low_remaining_quota_slows_the_feed_until_the_reset_and_reads_still_answer_from_cache`

`packages/tddy-github/tests/conditional_read_over_http.rs`:
- [x] `a_304_is_not_modified_and_keeps_the_poll_interval`
- [x] `rate_limit_headers_are_read_from_every_answer`
- [x] `gh_api_dash_i_304_reads_as_not_modified_despite_its_exit_code`
- [x] `check_runs_carry_status_timing_and_details_url`

`packages/tddy-github-inbox/tests/inbox_feed_acceptance.rs` (consumer seams):
- [x] `the_trunk_feed_reads_branch_commits_through_the_inbox_row` — the trunk watch's `TrunkPoller` keeps
  its semantics and reads through `InboxTrunkFeed`, so its existing six acceptance tests stay the
  behaviour proof and only `svc_trunk_watch_wiring.rs` changes
- [x] `repeated_reads_of_an_unchanged_pr_for_head_row_make_no_further_requests` — the property the
  `QueryBranch` PR leg relies on (N open views of one branch cost one read); the leg's re-wiring is
  proved by `project_stacks_rpc_acceptance.rs` staying green

## Decisions & Trade-offs

- **The trunk watch keeps the commits list as its source** — the 2026-11-21 trunk-watch changeset
  recorded "the commits list, not the events feed". The inbox serves that list as a conditional
  `branch_commits` row and invalidates it on `PushEvent`; the events feed drives PR, comment and push
  invalidations. **Decided by the operator (2026-11-22):** the trunk watch subscribes to a cached
  conditional commits row per repository and branch; the events feed drives PR, check and comment
  invalidations.
- **In-memory cache** — restart costs one unconditional read per row; persistence can follow if
  restarts prove costly.
- **A new crate** rather than growing `tddy-github` (whose `pr_api.rs` is already over budget).
- **Credentials**: the project's acting identity (vault account or local gh), resolved on each read
  the way the trunk watch already does — not the login-keyed retained token `pr_status_for_caller`
  uses today, which `ports.rs:556` already calls "the credential this stack retires".
- **Complexity 5** is the cohesive core of the stack's GitHub reads; splitting the cache from the
  feed would ship a cache nobody invalidates (a layer split).

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-22-github-event-inbox-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code (n2's scope; the PR-leg and stack-status migrations moved to n6 #1007)
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
