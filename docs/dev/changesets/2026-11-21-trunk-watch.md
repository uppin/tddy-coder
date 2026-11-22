# 2026-11-21 — New trunk commits reach every live orchestrator as activities

**Type:** Feature · **Stack:** `#orchestra` 5/9 (PR #983) · **Packages:** `tddy-github`,
`tddy-host-service`, `tddy-session-activity`, `tddy-daemon-kernel`, `tddy-session-lifecycle`,
`tddy-agent-launch`, `tddy-daemon-rpc`, `tddy-daemon`, `tddy-tool-engine`, `tddy-tools`,
`tddy-telegram-control` ·
Feature: [project-concept.md § Watching the trunk](../../ft/daemon/project-concept.md#watching-the-trunk),
[session-notifications.md](../../ft/daemon/session-notifications.md)

## Problem

Nothing told a running orchestrator that its project's trunk moved: its home went stale silently, and
so did the bases it handed to new sessions.

## What changed

- **`tddy-github::trunk_feed`** (new): `TrunkFeed`, a conditional read of a branch's newest 100 commits
  (ETag, `304`, `X-Poll-Interval`), over REST (`RestTrunkFeed`, its token read from a `TokenSource`
  on every read) and over `gh api -i` (`GhCliTrunkFeed`, a `304` read from the status line, not the
  exit code). `TrunkPoller` is one branch's polling rule as a step with no loop or clock: baseline
  first, new commits oldest first, a tip that does not descend from the last one a rewrite, the
  poll interval honoured, 60 s back-off after a failed read, an unconditional read every 15 min.
- **`tddy-host-service`**: `GithubCli::api_with_headers` — `gh api -i` as an OS user, a non-zero exit
  being an answer; `run` shared with `ask`.
- **`tddy-session-activity::trunk_watch`** (new): `TrunkWatcher` — one poller per project, built
  for its first subscriber through a `TrunkSource`, its subscribers swept through a
  `SubscriberLiveness` at every step and the poller dropped with its last one. Each change is
  published once per subscriber as a `TrunkCommit` `ACTIVITY` notification of that subscriber's
  own session. `TrunkSubscription` carries the subscribing caller's GitHub user.
- **`tddy-daemon-kernel`**: `SessionNotificationSource::TrunkCommit`, and
  `SessionNotificationSource::is_a_sessions_own` (message, trunk commit) — the sources no operator
  surface shows.
- **`tddy-session-activity`**: `WatchKind::TrunkCommit` (`trunk_commit`); a watch matches its owner's
  own trunk records and no other kind of its own session; the project log keeps `TrunkCommit`
  records though they are activity, and keeps them out of `ReadProjectNotifications`; the drawer's
  stream skips them.
- **`tddy-daemon-rpc`**: `CreateSessionWatch` admits a self-watch of `trunk_commit` alone; the
  idle-wake trigger takes trunk commits.
- **`tddy-session-lifecycle::svc_trunk_watch_wiring`** (new): `with_trunk_watch` /
  `spawn_trunk_watch` (one task stepping the watcher on its deadline, clamped to 1–5 s, the host's
  live sessions plus a 60 s start grace as its liveness); `HostTrunkSource` builds the feed from the
  project's `local_gh` (host `gh` as the OS user) or vault account (REST).
- **`tddy-agent-launch`**: `LaunchHost::orchestrator_running`, called after a CLI-agent start and a
  resume of an orchestrator-role session.
- **`tddy-daemon`**: the runtime installs the watch on the session host and starts its task.
- **`tddy-tool-engine`**, **`tddy-tools`**, **`tddy-telegram-control`**: the eighth watch kind in
  `WATCH_KINDS` and the tool and CLI descriptions; Telegram declines the source.

## Decisions

- **The commits list, not the events feed**: one conditional call answers both "what landed" and
  "was it rewritten". The PRD's events-feed plan was changed at the contract.
- **No new field on `SessionNotification`**: a trunk commit is a `TrunkCommit`-source notification
  whose `session_id` is the orchestrator and whose `text` is one line per commit.
- **Read through the self-watch**: the records are the orchestrator's own, so the inbox tools, which
  never return the caller's own session, do not list them. `ReadSessionActivity` reads a session's
  tool calls and is not a route to them; the PRD's mention of it was dropped.
- **Unsubscribe is the liveness sweep**: a stopped, ended or deleted session falls away within the
  driver's 5 s step; there is no separate stop hook.

## Found and fixed at wrap

- `TrunkCommit` records were published as `ACTIVITY`, which the project log declined, so a
  self-watch could never fire and an idle orchestrator was never woken. The log and the waker now
  take the source; an acceptance test drives a commit through the real log to a self-watch.
- The drawer's stream would have carried trunk commits; it skips them now, as it skips messages.
- The GitHub user of each subscribed session was kept in a map nothing pruned, and the start-grace
  entries likewise. The user now rides on the subscription and goes with it; expired grace entries
  are pruned every step.
- A rotated vault token needed a re-subscribe: `RestTrunkFeed` reads it on every poll now.

## Wrap-time measurements

- `trunk_feed.rs` 405, `trunk_watch.rs` 216, `svc_trunk_watch_wiring.rs` 294 production lines (new).
- `project_notification_log.rs` 498 → 497 (the source checks moved to `is_a_sessions_own`).
- No touched file crossed 500 production lines.

## Known limitations

- Only `github.com` remotes are watched — backlog: `2026-11-21-trunk-watch-reads-github-com-only.md`.
- A burst of more than 100 commits between two polls is reported as a rewrite.

## Tests

- `packages/tddy-github/tests/trunk_feed.rs` (8): baseline, new commits once oldest first, `304` and
  the poll interval, back-off, rewrite, the 15-minute sweep, both `gh` answers.
- `packages/tddy-session-activity/tests/trunk_watch_acceptance.rs` (6): one shared poller, a commit
  on every subscriber, stop with the last subscriber, unsubscribe, force-push, and the self-watch
  over the real log (with the inbox not listing it).
- `packages/tddy-session-activity/tests/session_notification_event_unit.rs`: the web stream never
  carries a trunk commit. `matcher.rs` `trunk_commit_tests` (3); `watch_wake.rs` (1).
- `packages/tddy-session-lifecycle/tests/orchestrator_session_acceptance.rs` (3): an orchestrator is
  subscribed at start and again at resume; an ordinary session is subscribed at neither.
  `trunk_watch_wiring_unit_tests.rs` (2): a project with no GitHub remote is left unwatched.
- `packages/tddy-daemon-rpc/tests/project_home_session_watch_acceptance.rs` (1): a self-watch of
  `trunk_commit` alone is admitted, any other self-watch refused.
