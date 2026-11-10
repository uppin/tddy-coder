# Daemon-run subagents and stream-only agents report their usage - PRD

**Date**: 2026-11-10
**PRD Type**: Enhancement
**Stack**: `#usage-binding 3/5` — branch `feature/usage-binding/live-tap`, on `#usage-binding 2/5`

## Affected Features

- **Primary Feature**: [Session Token Accounting](../session-token-accounting.md) — coding subagents
  and agents whose only usage source is their live protocol stream.

## Summary

Two kinds of conversation report zero today.

1. **Coding subagents the daemon runs for a session.** These are opened through
   `SessionAgentService` and run headless (`claude -p`, `codex exec`, ACP). From now on, each subagent
   conversation is recorded with the agent spec it ran, its conversation id and its agent home. The
   session's usage then evaluates **that spec's** `usage:` binding for it, as a child of the session's
   main conversation. A Claude subagent is read from its own transcript, so the same binding covers the
   main agent and the subagent.
2. **Agents with no files to read.** Cursor in headless mode, and any agent driven over ACP, report
   usage only in their protocol stream: stream-json `result`, ACP `usage_update`. The protocol driver
   now **appends those events, unmodified, as JSON lines** to
   `<session_dir>/usage/<conversation_id>.jsonl`. The agent's spec binds that file with the ordinary
   `jsonl` format, so the field mapping stays in YAML. This includes a reported cost: stream-json
   `total_cost_usd`, ACP `cost.amount`.

## Background

- Every protocol driver drops usage:
  - stream-json keeps only the text of `result`;
  - the Codex driver debug-logs its usage event;
  - ACP's mapping returns `None` for `usage_update`.
- A coding subagent's `cumulative_usage()` is hard-coded to zero, so the `subagent_list` rows the
  in-jail `tddy-tools` writes for them read 0.

## Proposed Changes

### What's Changing

- **The protocol drivers tap usage events.** Each driver writes the events its spec's
  `headless.usage_events` names, verbatim, to the session's usage file. The drivers are stream-json,
  codex-jsonl and ACP. Which events count is declared in YAML; the driver code names no field of them.
- **The daemon keeps a registry of agent conversations per session**: the spec id, the conversation id,
  the agent home, and the parent. It is persisted in the session directory, so a dormant snapshot still
  finds them.
- **The usage watcher evaluates each registered conversation's binding** and attaches the rows it
  yields under its parent.
- **`SubagentSession::cumulative_usage` reports the tapped totals**, instead of zero.
- **Bindings for the stream-only agents**:
  - `cursor.yaml`: the stream-json `result` usage;
  - `claude-acp.yaml` and `codex-acp.yaml`: the cost from `usage_update`, beside their transcript or
    rollout bindings.
- **A conversation two sources describe gets one row.** When two sources return the same conversation
  id, their rows merge field by field. A source supplies exactly the counters its mapping names, and two
  sources of one spec that map the same counter are refused at load.

### What's Staying the Same

- Claude's transcript binding and the daemon stream from `#usage-binding 1/5` and `2/5`.
- What each protocol driver returns to its caller.

## Impact Analysis

### Technical Impact

- `tddy-agent-backend` (the headless drivers and ACP mapping).
- `tddy-coding-subagent` (`cumulative_usage`).
- `tddy-session-agents` (the conversation registry).
- The daemon usage watcher (child conversations).
- `tddy-usage` (merging sources, owned counters).
- The specs (`headless.usage_events`, bindings for `cursor`, `claude-acp` and `codex-acp`).

### User Impact

- Subagents the session ran appear in the Usage tab with real numbers, nested under the main agent.
- Cursor sessions show usage.
- ACP agents show a cost.

## Implementation Plan

1. The tap, in each driver, with the spec-declared event filter.
2. The daemon conversation registry, persisted, and its evaluation in the watcher.
3. `cumulative_usage` from the tap.
4. Merging sources in `tddy-usage`.
5. Bindings for cursor and the two ACP specs.
6. Tests:
   - driver tests over recorded protocol lines;
   - a daemon test opening a coding subagent against a fixture agent and seeing its nested row;
   - merge refusals.

## Acceptance Criteria

- [ ] A coding subagent the daemon runs appears as a child row with its own tokens.
- [ ] A Cursor headless conversation reports the usage from its stream-json `result`.
- [ ] An ACP agent's reported cost appears on its row.
- [ ] Driver code names no usage field; the event filter and mapping are in YAML.
- [ ] Two sources claiming the same counter are refused at load.

## References

### Affected Features (Complete List)
- [Session Token Accounting](../session-token-accounting.md)

### Related Documentation
- Parent: `#usage-binding 2/5`.
