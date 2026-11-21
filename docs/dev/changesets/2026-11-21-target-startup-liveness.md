# 2026-11-21 — The startup liveness probe of a target agent has a round trip to answer

**Type:** Bug Fix · **Stack:** `#target-parity` 5/7 (PR #973) · **Packages:** `tddy-agent-launch` ·
Feature: [claude-cli-session.md § Agent on a target](../../ft/daemon/claude-cli-session.md)

## Problem

After an agent on a target recorded its pid, the start probed it with `kill -0` for the startup window
(500 ms), each probe cut off at what the window had left, and an unanswered probe counted as alive.
On a Coder workspace (6–9 s per call) no probe ever answered, so an agent that died at once was
reported started until its terminal ended (`timed_out` after 504 ms on `mantasi/remote-dev`).

## What changed

- **`remote_agent_launch/startup.rs`**: `exits_during_startup` bounds each probe by the start's
  `start_timeout`; the window passes only on an answered probe that found the agent alive, and returns
  a `StartupJudgement` (`Exited`, `Stayed`, `NeverAnswered`).
- **`remote_agent_launch.rs`**: new `RemoteLaunchError::AgentNeverAnswered { after }` (window +
  `start_timeout`), cleaned up like the other start failures (terminal stopped, no session).

## Decisions & Trade-offs

- **`start_timeout` as the probe's bound**: it already names the transport's connect time and the
  remote command's start (#963); no new setting.
- **Probing is capped at `startup_window + start_timeout`** from the pid's recording (orchestrator,
  2026-11-21, under "finish without approval"). Without it, a hung transport whose terminal never ends
  would be probed forever. A start with no answered probe by then **fails** with
  `AgentNeverAnswered`; it is not a fallback to "alive" (`unwrap_or(true)` stays gone).

## Tests

`remote_agent_launch_acceptance`:
`an_agent_that_dies_on_a_slow_target_fails_the_start_though_its_terminal_reports_late`,
`an_agent_that_stays_up_on_a_slow_target_starts` (guard),
`an_agent_on_a_target_that_never_answers_fails_the_start_after_its_bound`.
