# 2026-11-21 — A sandboxed or ACP child of an orchestrator is not told its back channel

**Category:** Deferred from `#orchestra` 2/9 (#980)
**Source:** `/validate-changes` on `feature/orchestra/child-back-channel`

`#orchestra` 2/9 gives every child of an orchestrator guidance naming its orchestrator and one back
channel ([project concept § Reporting to the orchestrator](../../ft/daemon/project-concept.md#reporting-to-the-orchestrator)).
It does so on the generic CLI start and its resume (`tddy-agent-launch`'s `cli_branch_starts.rs` →
`opencode_cli_spawn::start_cli_harness_session`, `cli_agent_steps::resumed_session_wiring`). Two other
start paths give a child nothing:

- **A sandboxed CLI child** (`svc_start_sandboxed_cli_harness_session`,
  `svc_start_sandboxed_claude_cli_session`). It records its orchestrator link but computes no
  `ReportsTo`. The jail also changes which channel is right: a jailed Claude does not share the
  host's Claude peers, so native peer messaging is probably not available, and `tddy-tools send`
  through the jail's session pointer is the likely channel. That needs checking in a real jail
  before the rule is written.
- **An ACP child** (`claude-acp`, `codex-acp` sessions, which start through `tddy-coder`). It has
  no first prompt the daemon writes, and no system-prompt file.

**Why deferred:** both paths are outside the node's `## Responsibility` (the generic CLI launch), and
the jailed channel choice needs a decision and a real-jail check that the node's tests cannot make.
**What would close it:** compute `ReportsTo` in both start paths, choose `ToolsSend` for a jailed
child (after the real-jail check), and prefix the first prompt (`back_channel::first_prompt_with`)
or the ACP session's first turn.
