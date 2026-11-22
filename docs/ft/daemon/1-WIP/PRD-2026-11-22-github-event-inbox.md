# GitHub event inbox — PRD

**Date**: 2026-11-22
**PRD Type**: Feature
**Stack**: `#workqueue` 2/14 — branch `feature/workqueue/github-event-inbox`, base `feature/workqueue/remove-pr-stack-recipe`

## Affected Features

- **Primary Feature**: [Project concept — watching the trunk](../project-concept.md)
- [PR stack live status](../../coder/pr-stack-live-status.md) — branch status reads come from the inbox
- [GitHub identity](../github-identity.md) — which credential reads a repository

## Summary

The daemon keeps **one GitHub inbox per repository**. It reads the repository's activity feed
conditionally (ETag, `304` costs nothing, `X-Poll-Interval` honoured), runs a periodic catch-up sweep
of recently updated pull requests, and polls check runs only for heads whose checks are still queued
or running. What it reads lands in a cache of rows — a pull request, the PR for a head branch, the
checks at a commit, a PR's review and issue comments, a PR's commits, a branch's newest commits —
each kept with its own ETag and revalidated conditionally. Events invalidate exactly the rows they
affect. Concurrent reads of one stale row make one request. In-process subscribers register for a
repository and the **event kinds** they care about (`pr`, `checks`, `comments`, `push`) and are told
when a matching row changes. The trunk watch and the stack's live branch status move onto it, so the
number of GitHub requests no longer grows with the number of open views or subscribed sessions.

## Background

Today the only conditional GitHub reader is the trunk feed. Every other read is an unconditional
`curl` with no cache; `ProjectStackView` polls `QueryBranch` every 5 s per rendered branch per open
browser view, each doing a `GET pulls?head=` — about two requests a second for a ten-node stack in a
single tab. Every agent and the daemon share the operator's 5,000 requests/hour; on 2026-11-21 one
session exhausted it in 32 minutes. Later `#workqueue` nodes add PR tabs, live checks on work items and
schedules that wait on checks — all of which must read through a shared, quota-aware path.

## Proposed Changes

### What's Changing

- New daemon component `GithubInbox`: per-repository feed task, row cache with per-row ETags,
  event → invalidation mapping, single-flight, subscriptions filtered by repository and event kind,
  rate-limit awareness (`X-RateLimit-Remaining`/`Reset`, `Retry-After`) that slows polling instead of
  failing readers.
- A conditional GitHub transport that returns status, ETag, poll interval and rate-limit headers, over
  both credential paths the daemon already uses (a project's vault account through REST, or the
  host's `gh` for a local-gh project).
- Check runs carry status (queued / in progress / completed), conclusion, start and completion times
  and a details URL.
- The trunk watch takes its commits from the inbox's branch-commits row; it keeps its own semantics
  (baseline, landed/rewritten activities, one poller per project).
- `QueryBranch`'s PR leg and the stack status tool read PRs from the inbox instead of a direct lookup.

### What's Staying the Same

- Trunk-watch activities and who subscribes (orchestrator start/resume).
- Agent PR reads (`pr_read`, `pr_comments`, `pr_search`) and all PR mutations call GitHub directly.
- github.com only (the enterprise gap stays recorded).

## Impact Analysis

### Technical Impact

A new crate holding the inbox and its ports; `tddy-github` gains the conditional transport and the
richer check-run shape; trunk watch, `QueryBranch` and stack status are re-wired; one daemon task per
repository with at least one subscriber or a recently read row.

### User Impact

No visible change other than fewer rate-limit failures; status in open stack views stays as fresh
(seconds, bounded by GitHub's poll interval) while costing a fixed amount per repository.

## Implementation Plan

One node, `feature/workqueue/github-event-inbox` (#workqueue 2/14).

## Acceptance Criteria

- [ ] Two subscribers of one repository share one feed; a `304` answer costs no further request and changes nothing.
- [ ] A pull-request event invalidates that PR's rows only; the next read revalidates with the stored ETag.
- [ ] Ten concurrent reads of one stale row make one GitHub request.
- [ ] A subscriber registered for `checks` is told when a pending check completes and is not told about comment-only changes.
- [ ] Checks are polled only while some head has queued or running checks.
- [ ] When the remaining quota is low the feed slows to the reset time, and cached reads keep answering.
- [ ] The trunk watch reports landed commits exactly as before, from the inbox's branch row.
- [ ] N open stack views of one branch cost the same GitHub requests as one.
