# New trunk commits reach every live orchestrator as activities - PRD

**Date**: 2026-11-21
**PRD Type**: Enhancement
**Stack**: `#orchestra` 5/9 — branch `feature/orchestra/trunk-watch`, base `feature/orchestra/base-freshness`

## Affected Features

- **Primary Feature**: [Project concept — orchestrator sessions](../project-concept.md)

## Summary

The daemon watches each project's remote trunk (`main`/`master`) **centrally**: one poller per project repository, started when the first live orchestrator of that project subscribes and stopped when no live session is subscribed. Orchestrator-role sessions subscribe automatically at start and resume. Each new trunk commit becomes a `trunk_commit` activity on every subscribed orchestrator. An orchestrator can watch (`CreateSessionWatch` on itself with kind `trunk_commit`) and query (`ReadSessionActivity` on itself) its own activities. GitHub is read through `gh` when the project uses local gh, otherwise through the REST API with the project's GitHub account.

## Background

Nothing tells a running orchestrator that the trunk moved; its home goes stale silently, and so do the bases it hands to new sessions.

## Proposed Changes

### What's Changing
- `TrunkWatcher`: subscribe/unsubscribe by session; one poll loop per repo; loop exits when the last live subscriber leaves (a stopped or ended session counts as gone).
- Polling is conditional: the repo events feed with an ETag (a 304 is free), honouring `X-Poll-Interval` (default 60 s), 60 s backoff after an error, and a catch-up compare of the trunk tip at start and every 15 min for what the feed misses.
- Transport per project: `local_gh` → `gh api -i` (304 exits non-zero and is read as success); otherwise REST with the project's vault GitHub token.
- Each new commit → one `trunk_commit` activity (sha, author, subject, pushed_at) per subscriber; a forced push is reported as a `trunk_rewritten` activity with the old and new tip.
- Orchestrators may create a watch on **themselves** of kind `trunk_commit`, and read their own activity.

### What's Staying the Same
- Non-orchestrator sessions are never subscribed.
- GitHub identity rules (`local_gh` vs vault account).

## Impact Analysis

### Technical Impact
- **tddy-github**: `TrunkFeed` port with conditional reads (ETag, 304, `X-Poll-Interval`) over REST (vault account) and over `gh api -i` (local gh)
- **tddy-host-service**: the `gh` transport of `TrunkFeed`
- **tddy-session-activity**: `TrunkCommit` activity source, a `trunk_commit` watch kind, and an orchestrator's self-watch
- **tddy-daemon-rpc**: `TrunkWatcher` — one poller per (project, remote, trunk), subscriptions by orchestrator session, stop when none is live
- **tddy-agent-launch**: subscribe on orchestrator start / resume, unsubscribe on stop

### User Impact
An orchestrator learns within about a minute that trunk moved, and can refresh (n4) before starting more work.

## Implementation Plan

One node: `feature/orchestra/trunk-watch` (#orchestra 5/9).

## Acceptance Criteria

- [ ] Two live orchestrators of one project share one poller.
- [ ] A new trunk commit lands as a `trunk_commit` activity on both.
- [ ] The poller stops when the last subscribed orchestrator stops.
- [ ] A 304 answer produces no activity and no extra request before `X-Poll-Interval`.
- [ ] A local-gh project polls through `gh`; a vault-account project through REST.
- [ ] An orchestrator's self-watch of kind `trunk_commit` fires on the new commit.
