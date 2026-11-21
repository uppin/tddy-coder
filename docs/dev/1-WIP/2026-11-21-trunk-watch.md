# Changeset: New trunk commits reach every live orchestrator as activities

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — new central poller with lifecycle tied to session liveness, two GitHub transports with conditional requests, a new activity source and watch kind
**Stack**: `#orchestra` 5/9 — branch `feature/orchestra/trunk-watch`, base `feature/orchestra/base-freshness`

## Initial Discovery

[2026-11-21-trunk-watch-initial-discovery.md](./2026-11-21-trunk-watch-initial-discovery.md)

## Affected Packages

- **tddy-github**: `TrunkFeed` port with conditional reads (ETag, 304, `X-Poll-Interval`) over REST (vault account) and over `gh api -i` (local gh)
- **tddy-host-service**: the `gh` transport of `TrunkFeed`
- **tddy-session-activity**: `TrunkCommit` activity source, a `trunk_commit` watch kind, and an orchestrator's self-watch
- **tddy-daemon-rpc**: `TrunkWatcher` — one poller per (project, remote, trunk), subscriptions by orchestrator session, stop when none is live
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

The first push of this PR, before its implementation: `TrunkFeed` trait + `Conditional`, `TrunkWatcher::{subscribe, unsubscribe, live_pollers}`, `ActivitySource::TrunkCommit`, `WatchKind::TrunkCommit`; failing tests below with a hand clock and a fake feed.

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

`packages/tddy-daemon-rpc/tests/trunk_watch_acceptance.rs`:
- [ ] `two_live_orchestrators_of_one_project_share_one_poller`
- [ ] `a_new_trunk_commit_lands_as_an_activity_on_every_subscribed_orchestrator`
- [ ] `the_poller_stops_when_the_last_subscribed_orchestrator_stops`
- [ ] `an_orchestrators_self_watch_fires_on_a_new_trunk_commit`
- [ ] `a_forced_push_is_reported_as_a_rewritten_trunk`

`packages/tddy-github/tests/trunk_feed.rs`:
- [ ] `a_not_modified_answer_produces_nothing_and_waits_out_the_poll_interval`
- [ ] `a_failed_read_backs_off_before_the_next_poll`
- [ ] `the_gh_transport_reads_a_304_as_not_modified`

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
