# 2026-11-21 — An orchestrator's child is told how to report back, and a target child's report reaches it

**Type:** Feature · **Stack:** `#orchestra` 2/9 (PR #980) · **Packages:** `tddy-agent-launch`,
`tddy-agent-backend`, `tddy-coding-agent-specs`, `tddy-session-lifecycle` · Feature:
[project-concept.md § Reporting to the orchestrator](../../ft/daemon/project-concept.md#reporting-to-the-orchestrator)

## Problem

A session started by an orchestrator learned nothing about it. The notify guidance covered only
`notify` and `rename-session`, `PostSessionMessage` was reachable only by agents that can run
`tddy-tools`, and the remote hook follower dropped any event it did not know, so a child on a Coder
or SSH target could not report at all.

## What changed

- **`tddy-agent-launch`**: `back_channel` chooses one channel per child: native peer messaging by
  name (both agents declare `coding_agent.peer_name`, same host), `tddy-tools send`, or a
  `TddyMessage` hook-log line on a target. It renders the guidance and the peer names
  (`tddy-<session id>`, `claude --name`). The guidance goes into the operator-notify system prompt,
  where there is one (start and resume). Otherwise it goes ahead of the first prompt (`first_prompt_with`).
  A target child whose agent installs no hooks is minted a hook token. The follower accepts
  `TddyMessage` (`remote_hook_message`) and posts it through
  `HookReportDelivery::post_message_to_orchestrator`.
- **`tddy-agent-backend`**: `REMOTE_MESSAGE_EVENT` (`"TddyMessage"`).
- **`tddy-coding-agent-specs`**: `coding_agent.peer_name` (`PeerNameSlot`, `claude.yaml`:
  `--name`), and `codex.yaml`'s run argv takes the first prompt as the positional `[PROMPT]`. A
  `codex-cli` start used to drop its prompt.
- **`tddy-session-lifecycle`**: `post_remote_message_to_orchestrator` publishes a target child's
  report as the `message` record `PostSessionMessage` writes.

## Restructuring (`/pr-wrap` step 3.5)

| File | Production lines | Now |
|---|---:|---|
| `tddy-agent-launch/src/remote_hook_follower.rs` | 517 (base) → 542 → **386** | delivery side → `remote_hook_delivery.rs` (170), re-exported |
| `tddy-agent-launch/src/cli_agent_steps.rs` | 494 → 537 → **364** | resume wiring → `cli_agent_steps/resumed_wiring.rs` (193) |
| `tddy-agent-launch/src/opencode_cli_spawn.rs` | 505 → 522 → **445** | small start steps → `opencode_cli_spawn/start_steps.rs` |
| `tddy-coding-agent-specs/src/spec.rs` | 500 → 519 → see the package entry | |

The `tddy-agent-launch` moves were made by hand. `tddy-tools restructure` refused every `move_item` in
that crate on an unrelated `#[path]`-attributed `mod tests;` (`restore_tabs.rs`). Closed code issue
`oversized-file-remote-hook-follower` (517 at detection, 542 at its peak here, **386** at close).

## Decisions

- Both orchestrator roles (`project-home` and `orchestrator`) report and are named alike.
- Native Claude↔Claude messaging is preferred on one host. Remote reports ride `hooks.log`, not
  a separate outbox file.
- An agent with no system-prompt slot is told in its first prompt (developer, 2026-11-21).

## Tests

`tddy-agent-launch/tests/child_back_channel_acceptance.rs` (8),
`tddy-session-lifecycle/tests/child_back_channel_launch_acceptance.rs` (5, including
`a_codex_child_is_told_in_its_first_prompt_to_report_to_its_orchestrator_with_tddy_tools_send` and
the resume guard), `tddy-coding-agent-specs/tests/spec_peer_name_acceptance.rs` (2), and the
`cli_agent_steps` wiring unit tests (Codex on a target, Claude's system prompt, `first_prompt_with`).

## Left open

[2026-11-21-back-channel-for-sandboxed-and-acp-children](../todo/2026-11-21-back-channel-for-sandboxed-and-acp-children.md).
