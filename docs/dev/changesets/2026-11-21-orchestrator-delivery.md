# 2026-11-21 — An orchestrator reaches each child by its best route and releases a terminal it claimed

**Type:** Feature · **Stack:** `#orchestra` 7/9 (PR #985) · **Packages:** `tddy-tool-engine`, `tddy-daemon-rpc`, `tddy-cli-sessions`, `tddy-terminal-rpc`, `tddy-session-lifecycle`, `tddy-coder`, `tddy-coding-agent-specs`, `tddy-tools` ·
Feature: [project-concept.md § Sending a session a turn](../../ft/daemon/project-concept.md#sending-a-session-a-turn), [§ Typing into a terminal](../../ft/daemon/project-concept.md#typing-into-a-terminal)

## Problem

An orchestrator could not tell how best to message each child; only Claude took a terminal turn;
`ClaimSessionTerminal` had no release, and the home's lease table only grew.

## What changed

- **`tddy-tool-engine::delivery_route`** (new): `DeliveryRoute { Native { peer_name }, Terminal, Inbox }`
  and `delivery_route_for(DeliveryInputs)` — a target (Coder/SSH) is `terminal`; a local child whose
  agent and orchestrator both have `coding_agent.peer_name` is `native`; anything else `inbox`.
  `ListedSession.delivery_route` carries it (`null` from an older peer).
- **`ReleaseSessionTerminal`** (`project_home_terminal_tools.rs`): gives back this home's own claim;
  `ProjectHomeTerminalPort::release_terminal` (default refuses). Added to the catalogue, the
  target-host forwarding list, the read-only allow list and `tddy-tools`' admission.
- **`TerminalControl::release`** (`tddy-terminal-rpc` port, no new RPC), implemented by
  `CliManagerTerminalControl` → `CliSessionManager::release_control` (broadcasts an empty holder) and by
  the coder participant (holds nothing, always released).
- **Leases die with their session:** `CliSessionManager::forget_control` on `stop_session`, and
  `DetachWatch::drop_agent_state` drops the lease when an agent ends. `HomeTerminalLeases::following`
  reads the host's `agent_ends()` before each access and forgets every claim on an ended session;
  `forget` / `forget_target` remove one or all.
- **`cursor.yaml`, `muse.yaml`**: `turn_input: { kind: terminal_paste, submit_key: Enter }`.

## Decisions

- No `ReleaseTerminalControl` RPC (the PRD's first sketch): release is daemon-internal through the
  existing `TerminalControl` port, so no proto, TS or web change.
- `delivery_route_for` takes the peer name as input (`native_peer_name_of`, computed by the daemon):
  `tddy-tool-engine` cannot depend on `tddy-agent-launch`.
- A release forgets the remembered token whatever the outcome: a token the lease refuses is stale.
- A lagged agent-end subscriber may leave a stale token; harmless, since every use verifies it
  against the live lease.
- Cursor/Muse paste behaviour is declared, not observed live:
  [`docs/dev/todo/2026-11-21-cursor-muse-terminal-paste-unverified.md`](../todo/2026-11-21-cursor-muse-terminal-paste-unverified.md).

## Tests

`packages/tddy-daemon-rpc/tests/orchestrator_delivery_acceptance.rs` (3, real `ExecuteTool`, a live
stand-in terminal), `packages/tddy-coding-agent-specs/tests/spec_turn_input_delivery_acceptance.rs` (2),
unit tests in `tddy-tool-engine::delivery_route` (5), `tddy-cli-sessions` `control_lease::release_tests`
(4) and `tddy-daemon-rpc` `home_terminal_leases::release_tests` (2);
`tddy-tool-engine/tests/project_home_target_host.rs` now counts `ReleaseSessionTerminal` among the
target tools (20).
