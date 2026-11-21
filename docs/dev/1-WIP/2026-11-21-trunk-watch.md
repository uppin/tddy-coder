# Changeset: New trunk commits reach every live orchestrator as activities

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — new central poller with lifecycle tied to session liveness, two GitHub transports with conditional requests, a new activity source and watch kind
**PR**: [#983](https://github.com/uppin/tddy-coder/pull/983)
**Stack**: `#orchestra` 5/9 — branch `feature/orchestra/trunk-watch`, base `feature/orchestra/base-freshness`

## Initial Discovery

[2026-11-21-trunk-watch-initial-discovery.md](./2026-11-21-trunk-watch-initial-discovery.md)

## Affected Packages

- **tddy-github**: `TrunkFeed` port with conditional reads (ETag, 304, `X-Poll-Interval`) over REST (vault account) and over `gh api -i` (local gh)
- **tddy-host-service**: the `gh` transport of `TrunkFeed`
- **tddy-session-activity**: `TrunkCommit` activity source, a `trunk_commit` watch kind, and an orchestrator's self-watch
- **tddy-session-activity**: `TrunkWatcher` — one poller per project, subscriptions by orchestrator session, stop when none is live
- **tddy-daemon-kernel**: `SessionNotificationSource::TrunkCommit`
- **tddy-telegram-control**, **tddy-tool-engine**, **tddy-tools**: skip / list the new source and kind
- **tddy-daemon-rpc**: drives the watcher and builds its `TrunkSource`
- **tddy-agent-launch**: subscribe on orchestrator start / resume, unsubscribe on stop

## Related Feature Documentation

- PRD: [PRD-2026-11-21-trunk-watch.md](../../ft/daemon/1-WIP/PRD-2026-11-21-trunk-watch.md)

## Summary

The daemon watches each project's remote trunk (`main`/`master`) **centrally**: one poller per project repository, started when the first live orchestrator of that project subscribes and stopped when no live session is subscribed. Orchestrator-role sessions subscribe automatically at start and resume. Each new trunk commit becomes a `trunk_commit` activity on every subscribed orchestrator. An orchestrator can watch (`CreateSessionWatch` on itself with kind `trunk_commit`) and query (`ReadSessionActivity` on itself) its own activities. GitHub is read through `gh` when the project uses local gh, otherwise through the REST API with the project's GitHub account.

## Responsibility

- `TrunkFeed` port and both transports; `TrunkWatcher` and its lifecycle.
- `trunk_commit` / `trunk_rewritten` activity source, the `trunk_commit` watch kind and self-watch admission.
- Auto-subscribe on orchestrator start/resume.

## Boundaries

- Does not refresh the home worktree (n4 owns `RefreshHome`; the orchestrator decides).
- No web view of the watcher.
- No webhooks.

## Dependencies

None inside `#orchestra`. It builds on its base, `#target-parity` 7/7 (#975), and on what is on master.

## Draft PR contract

The first push of this PR, before its implementation: `TrunkFeed` + `Conditional` + `TrunkPoller` (tddy-github), `TrunkWatcher::{subscribe, unsubscribe, live_pollers, step}` (tddy-session-activity), `SessionNotificationSource::TrunkCommit`, `WatchKind::TrunkCommit`; failing tests below with a hand clock and a scripted feed.

## Green wave

**Wave:** 1 of 4
**Greenable independently:** yes
**Concurrent with:** n1, n2, n3, n4
**Blocks:** n9

Real dependency edges (the line is n1 · n2 · n3 · n4 · n5 · n6 · n7 · n8 · n9):

    n1 path-reach → n6 access-elicitation, n8 orchestrator-lsp
    n6 access-elicitation → n8 orchestrator-lsp
    n2 child-back-channel → n7 orchestrator-delivery
    n1 … n8 → n9 orchestrator-prompt

Waves: 1 = n1, n2, n3, n4, n5 · 2 = n6, n7 · 3 = n8 · 4 = n9 (run `1 1 1 1 1 2 2 3 4`).
Inside wave 1, blockers lead: n1 (3 transitive dependents: n6, n8, n9), n2 (2: n7, n9), then n3, n4, n5
(1 each: n9) in foundational order. Inside wave 2: n6 (2: n8, n9) before n7 (1: n9). n3–n5 and n7 sit
in the line only because `gh stack` needs a line; none depends on its neighbour.

## Prerequisites

### ⚠ DURING — [oversized-file-pr-api.md](../../../packages/tddy-github/docs/code-issues/oversized-file-pr-api.md), [dead-code-github-pr-mock-transport.md](../../../packages/tddy-github/docs/code-issues/dead-code-github-pr-mock-transport.md)

The feed goes in its own module; `pr_api.rs` does not grow, and the mock transport is not reused.

### Unrelated — [2026-11-16-session-watches-are-not-shown-on-the-web.md](../todo/2026-11-16-session-watches-are-not-shown-on-the-web.md)

The new kind will be invisible on the web for the same reason as every other kind.

## Scope

- [ ] Central `TrunkWatcher` with subscribe/unsubscribe and stop-when-unsubscribed
- [ ] Conditional polling over `gh` and REST, chosen by `local_gh`
- [ ] `trunk_commit` activities, watch kind and orchestrator self-watch
- [ ] Feature docs `project-concept.md` § Watching sessions and `session-notifications.md` updated through the wrap

## Technical Changes

### State A (Current)

No remote trunk polling anywhere; watches target other sessions only; `ProjectRecord.local_gh` (`packages/tddy-projects/src/project_storage.rs:50`) picks the identity.

### State B (Target)

Every live orchestrator receives trunk commits as its own activities.

## Acceptance Tests

`packages/tddy-session-activity/tests/trunk_watch_acceptance.rs` (scripted GitHub, recording bus, hand clock):
- [ ] `two_live_orchestrators_of_one_project_share_one_poller`
- [ ] `a_new_trunk_commit_lands_as_an_activity_of_every_subscribed_orchestrator`
- [ ] `the_poller_stops_when_the_last_subscribed_orchestrator_stops`
- [ ] `an_unsubscribed_orchestrator_gets_no_more_trunk_activities`
- [ ] `a_forced_push_is_reported_as_a_rewritten_trunk`

`packages/tddy-github/tests/trunk_feed.rs`:
- [ ] `the_first_poll_is_a_baseline_and_reports_nothing`
- [ ] `new_commits_are_reported_once_oldest_first`
- [ ] `a_not_modified_answer_reports_nothing_and_waits_out_the_poll_interval`
- [ ] `a_failed_read_backs_off_before_the_next_poll`
- [ ] `a_tip_that_does_not_descend_from_the_last_one_is_a_rewrite`
- [ ] `every_fifteen_minutes_the_read_ignores_its_etag`
- [ ] `the_gh_transport_reads_a_304_as_not_modified`
- [ ] `the_gh_transport_reads_commits_and_their_etag`

Unit — `packages/tddy-session-activity/src/session_watches/matcher.rs` (`trunk_commit_tests`):
- [ ] `a_trunk_record_is_a_trunk_commit_event`
- [ ] `an_orchestrators_self_watch_matches_its_own_trunk_activity`
- [x] `a_self_watch_still_matches_no_other_kind_of_its_own_session` — regression guard

The surface they specify (bodies `TODO(trunk-watch)`): `tddy_github::trunk_feed::{TrunkFeed, Conditional, TrunkCommit, RepoSlug, TrunkReadError, RestTrunkFeed, GhApi, GhApiOutput, GhCliTrunkFeed, TrunkPoller, TrunkChange, PollOutcome, DEFAULT_POLL_INTERVAL_MS, ERROR_BACKOFF_MS, SWEEP_EVERY_MS}`, `tddy_session_activity::trunk_watch::{TrunkWatcher, TrunkSubscription, TrunkSource, SubscriberLiveness, trunk_activity_text}`, `SessionNotificationSource::TrunkCommit` (stored, no wire form, skipped by Telegram), `WatchKind::TrunkCommit` / `"trunk_commit"` in `WATCH_KINDS`.

**Contract-time decisions:** a trunk commit travels as a `TrunkCommit`-source notification whose `session_id` is the subscribed orchestrator and whose `text` is one line per commit — no new field on `SessionNotification`, which has 119 struct literals. The poll reads the trunk's newest commits conditionally (ETag), not the repository events feed: one call answers both "what landed" and "was it rewritten". The daemon's wiring — subscribe on orchestrator start and resume, the step task, and the `TrunkSource` building `GhCliTrunkFeed` (host `gh` as the OS user) or `RestTrunkFeed` (vault token) from `ProjectRecord.local_gh` — is implementation; `/green` adds an orchestrator-start test for it.

## Decisions & Trade-offs

- Both orchestrator roles (`project-home` main and `orchestrator`) get this change: they share
  `is_orchestrator_role` and one wiring (developer, 2026-11-21).
- The stack sits on `#target-parity` 7/7 (#975), because that stack rewrites the remote hook path
  and answer files this stack builds on (developer, 2026-11-21).
- Orchestrators subscribe automatically at start; the poller stops when no live session is subscribed (developer, 2026-11-21).
- Transport follows the project's `local_gh` setting (developer, 2026-11-21).

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-21-trunk-watch-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (delegated: the user said "Finish the remaining ones without stopping")
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
