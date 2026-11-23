# 2026-11-22 — One conditional GitHub feed per repository; the trunk watch reads through it

**Type:** Feature · **Stack:** `#workqueue` 2/14 (PR #1003) · **Packages:** `tddy-github-inbox` (new),
`tddy-github`, `tddy-session-lifecycle` (and test fixtures in `tddy-daemon-rpc`, `tddy-workflow-recipes`) ·
Feature: [project-concept.md § Watching the trunk](../../ft/daemon/project-concept.md#watching-the-trunk) ·
Design: [tddy-github-inbox/docs/architecture.md](../../../packages/tddy-github-inbox/docs/architecture.md)

## Problem

The trunk feed was the daemon's only conditional GitHub reader; every other read was an unconditional `curl`
with its response headers discarded, and the stack view's 5 s `QueryBranch` poll cost one `GET pulls?head=`
per branch per open view. Every agent and the daemon share the operator's 5,000 requests/hour, and later
`#workqueue` nodes (PR tabs, live checks, schedules) all read PR state.

## What changed

- **`tddy-github-inbox`** (new): `GithubInbox` — rows (`Pr`, `PrForHead`, `ChecksAt`, `Conversation`,
  `PrCommits`, `BranchCommits`) cached with one ETag per request and revalidated conditionally; single-flight
  per row; a per-repository events feed (baseline, then `PullRequestEvent` / `PushEvent` / comment and review
  events invalidate their rows), a 30 s pending-check poll while any cached head has unfinished runs, and a
  15-minute updated-pulls sweep; pacing by `X-Poll-Interval`, `Retry-After` and the rate limit (paused below
  `LOW_QUOTA_REMAINING`, reads answer from cache); kind-filtered subscriptions; a 30 s TTL for rows of a
  repository nobody subscribes to; `InboxTrunkFeed`. Conversation rows carry GitHub's id on every review and
  comment.
- **`tddy-github`**: `conditional` (`ConditionalRead`, `RestConditionalRead`, `GhCliConditionalRead`,
  `GITHUB_API_BASE`); `pr_json` and `check_runs` parsers, which `RealGithubPrApi`'s insight reads now delegate
  to; `CheckRun` gains `id`, `status`, `started_at`, `completed_at`, `details_url`; `PrReview` and
  `PrIssueComment` gain `id`; `RestTrunkFeed` / `GhCliTrunkFeed` removed (the conditional readers replace
  them, each path keeping its tests in `conditional_read_over_http.rs`).
- **`tddy-session-lifecycle`**: the trunk-watch wiring owns the inbox, steps it, registers each project's
  reader (`HostRepoReaders`) and hands `TrunkPoller` an `InboxTrunkFeed`; the trunk watch's own semantics and
  its six acceptance tests are unchanged.

## Moved out

`QueryBranch` / `GetPrStatus`'s PR leg and the stack `status` tool reading through the inbox moved to n6
(#1007) by the operator's decision: it needs repo→project credential plumbing into `PrStackRpcHandler` and a
sync→async adapter for `GithubPrApi`.

## Measurements and backlog

- `packages/tddy-github/docs/code-issues/oversized-file-pr-api.md`: 1,017 → 954 production lines (total
  1,428 → 1,365) — partially reduced, record kept.
- Added: `docs/dev/todo/2026-11-22-github-inbox-reads-a-repository-with-one-projects-credential.md`.
- Narrowed: `2026-11-21-trunk-watch-reads-github-com-only.md` (the REST API base is a constructor argument
  now), `2026-07-30-pr-stack-full-control-follow-ups.md` (the parsing half of the transport-seam item).
- Untouched, owned by n6: `2026-07-26-pr-stack-status-polling-and-stack-hygiene.md`.

## Known limits

In-memory only; one events page (100) per poll; github.com only; the trunk-watch loop does not yet sleep on
the inbox's `next_step_at_unix_ms` (no subscriber exists yet, so `step` reads nothing).

## Verification

Scoped: `cargo test -p tddy-github` (`conditional_read_over_http` 7, `trunk_feed` 6, unit 79, …),
`-p tddy-github-inbox` (`inbox_feed_acceptance` 17), `tddy-session-lifecycle --test
orchestrator_session_acceptance` 8, `tddy-session-activity --test trunk_watch_acceptance` 6; clippy
`-D warnings` on the five touched packages.
