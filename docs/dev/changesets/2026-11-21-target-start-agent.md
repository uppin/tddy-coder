# 2026-11-21 — A target session starts without the agent on the daemon host

**Type:** Feature · **Stack:** `#tab-hardening` 4/4 (PR #992) · **Packages:** `tddy-agent-launch`,
`tddy-session-lifecycle` (tests) ·
Feature: [claude-cli-session.md](../../ft/daemon/claude-cli-session.md#the-generic-cli-spec-launch-serves-claudes-needs),
[remote-managed-worktree.md](../../ft/daemon/remote-managed-worktree.md#agent-on-the-target-startsessionrequesttarget)

## Problem

A start or resume on a target (ssh or Coder) resolved the agent's binary on the **daemon's host** first and
refused with "tool claude is unavailable" when the host had none, although the agent runs on the target. The
new-tab half was fixed in `#session-liveness` 8/8; the session half stayed a todo.

## What changed

- **`tddy-agent-launch/src/target_start_agent.rs`** — `resolve_session_agent_for_target(agents, kind)` reads the
  spec with the bare command as the tool's `command` and `binary`
  (`HostCodingAgents::resolve_for_target`, `spec_launch.rs`); the `tddy-tools` closure the daemon runs itself stays
  host-resolved. `require_the_agent_on_the_target(shell, agent)` is one `probe_agent_on_target` (1/4) from the login's
  own directory, mapped by `tab_launch_status`. `LaunchSessions::agent_to_launch(kind, &target_shell)` dispatches
  host or target; `start_target_shell(req)` resolves the start's target shell before anything is created.
- **`start_cli_harness`** — `refuse_a_target_combination`, then the target shell, then the agent (probed), then the
  other refusals, the prelude and the worktree cut. **`resume_cli_harness`** settles its agent the same way before
  it re-wires anything.
- A missing agent fails the start or resume `FAILED_PRECONDITION` naming the command; an unreachable target
  `UNAVAILABLE` with the transport's words; nothing is cloned, cut or recorded.
- Tests: `tddy-agent-launch` `target_start_agent_tests` (five resolution cases, three probe cases);
  `tddy-session-lifecycle` `remote_target_session_acceptance` (start and resume with no agent on the host, a missing
  agent fails before any cut) and `coder_target_session_acceptance` (the Coder start), sharing
  `tests/host_without_agent_support/`.

## Decisions & Trade-offs

- **Resolve on the target, probe first** (developer decision), as target agent tabs do.
- **The probe runs from the login's own directory (`.`)** at start and resume: a start has no worktree there yet,
  and a resume whose worktree is gone must be reported by its launch, not misread as a missing agent.
- **`CliSessionAgent` is not split into a host and a target type**: a target agent's `tool.binary` is the bare
  command, which every target path already treats it as. A separate type would touch every reader for no
  behaviour change.
- **`refuse_a_target_combination` moves ahead of the agent's resolution**, so a conflicting request is refused before
  the target is asked anything.
- **The start resolves its target shell twice** (`start_target_shell`, and inside `start_cli_harness_session`) to
  keep two open complexity records from growing; the resolution makes no remote call.
- **One extra round trip per start and resume** (about 6–8 s on Coder), accepted; pinned on a stand-in target only
  ([todo](../todo/2026-11-16-coder-hosts-never-run-against-a-real-deployment.md)).
- **`agent_to_launch` takes `&Option<Arc<dyn RemoteShell>>`** so the resume's agent statement stays one line
  (`resume_cli_harness` is an open complexity record; it is unchanged at 84 lines).

## Backlog

`docs/dev/todo/2026-11-19-a-target-start-requires-the-agent-on-the-daemon-host.md` is deleted: option 3 (resolve on
the target, probe first) is built for start and resume, and `tddy-tools` stays host-resolved.
