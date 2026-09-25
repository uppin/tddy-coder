# Changeset: tddy-session-lifecycle becomes a wiring crate

**Date**: 2026-09-23
**Status**: 🚧 In Progress — green
**Type**: Refactor (crate extraction; no behaviour change)
**Stack**: `#carve` 15/15, on top of `lifecycle-wiring` (#524, the destructure node)

## Initial Discovery

[2026-09-23-carve-lifecycle-wiring-initial-discovery.md](./2026-09-23-carve-lifecycle-wiring-initial-discovery.md),
from #524. The receivers and the port design below were checked against every `Cargo.toml` on
2026-09-23. Host coupling was measured at function level.

## Affected Packages

- **`tddy-session-lifecycle`**: left with only host construction, delegating port impls and `pub use`
  facades.
- **New crates:** `tddy-agent-launch`, `tddy-session-split`.
- **Existing receivers:**
  - `tddy-session-agents`, `tddy-session-files`, `tddy-session-activity` and `tddy-session-catalog`;
  - `tddy-terminal-rpc`, `tddy-daemon-sandbox`, `tddy-daemon-livekit` and `tddy-daemon-kernel`;
  - `tddy-demo-runner` (or `tddy-vm`).
- **Consumers** (`tddy-daemon-rpc`, `tddy-daemon`, `tddy-telegram-control`): none is edited. They
  resolve through facades.

## Related Feature Documentation

None. This is a behaviour-preserving extraction, so there is no PRD.

## Summary

After #524, `tddy-session-lifecycle` is still ~20k production lines, but every file is under 500,
every function is under 150 lines, and nothing is duplicated.

This PR moves each topic to a receiver below lifecycle:
- each receiver defines a state struct and a callback trait;
- lifecycle builds the struct from its fields and implements the trait;
- what remains is **wiring**: the host struct, its builders, the port impls, and the facades.

Every public `tddy_session_lifecycle::…` path stays reachable.

## Background

This is the last `#carve` node. It carries the stack's acceptance criterion for lifecycle, which the
developer tightened on 2026-09-23 from ≤ 10k to **wiring only**. It runs after #524, because a crate
move cannot shrink what #524 restructures.

## Responsibility

- Cut the three cross-topic cycles (see "Phase 2 design").
- Define each receiver's state struct and callback trait; lifecycle builds and implements them.
- Move the topics leaves-first, in the order below. Tests move with their code.
- Leave a facade for every public `tddy_session_lifecycle::…` path.
- Reach the wiring size target the developer picks (see "What wiring only can reach").

## Boundaries

- **No behaviour change.** #524's baseline is re-run with zero regressions after every move.
- **No receiver depends on `tddy-session-lifecycle`**, and none goes over 10k production lines.
- **No consumer crate is edited** except where a test reads lifecycle source by path; each such case
  is listed.
- **The `PeerRouted*` wrappers stay in lifecycle** unless the developer approves moving routing down
  behind a forwarding port.
- **Nothing is restructured here.** Code moves as #524 left it; a seam that needs cutting is a
  finding against #524, not work done here.
- **Nothing is moved into `tddy-daemon-rpc`.** It depends on lifecycle, so a facade over it is
  impossible.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `13` destructure (#524) | every lifecycle file < 500, no function > 150, the DRY inventory done, misplaced code relocated, the baseline | moves the files #524 laid out, as they are | split, deduplicate or re-measure anything; that is #524's |
| `12` core-split (#522) | `tddy-core` facades | lifecycle and the receivers keep naming `tddy_core::…` | touch `tddy-core` or the nine crates |

## Draft PR contract

This is a mechanical extraction, one of the pr-stack skill's two named exceptions. The draft is this
plan. If the developer approves shape tests, they are the contract, following #522's
`core_facade_shape.rs`:
- lifecycle meets the wiring definition;
- every receiver is ≤ 10k and none depends on lifecycle;
- the named items are defined in the named receivers.

## Green wave

**Wave:** after #524.
**Greenable independently:** **no.** It moves the layout #524 creates, so it can go green only once
#524 is green and this branch is rebased onto it.
**Concurrent with:** nothing.
**Blocks:** nothing. This is the top of the stack.

## Prerequisites

| Item | Verdict | What this change does about it |
|---|---|---|
| #524 green (layout, DRY, code issues) | ✅ merged to master (`2688227f`) | This branch sits on it |
| The wiring size target | ✅ decided: **~3.4k** (developer, 2026-09-25) | `test_util` gated or moved to a testkit, `service_util` moved down |
| The `PeerRouted*` forwarding port | ✅ not built | The ~3.4k target keeps `PeerRouted*` in lifecycle |
| Shape tests as the contract | ✅ decided: **none** (developer, 2026-09-25) | The plan is the contract; #524's baseline, re-run after every move, is the gate |
| [2026-09-09 untested complexity hotspots](../todo/2026-09-09-tddy-daemon-untested-complexity-hotspots.md) | ⚠ **DURING** | Re-measure per owning crate after the moves |

## Scope

- [x] Rebased onto a green #524 (merged; branch rebuilt on master `2688227f`, dropping the already-wrapped #524/#527 WIP docs)
- [ ] The three cross-topic cycles cut: agent-def resolution moves down into `tddy-session-agents` (T1↔T3); a `SplitHost` port (T1↔T4); callback ports for T3→T2 and T4→T2
- [ ] Per-topic state structs + callback traits, defined in each receiver (the inverted pattern)
- [ ] Topics moved leaves-first (order below); receivers ≤ 10k, none depends on lifecycle
- [ ] `tddy-session-lifecycle` meets the wiring definition; facades cover every public path; consumers unedited
- [ ] Tests moved with their code; baseline back to #524's numbers
- [ ] `/analyze-code-issues` on `tddy-agent-launch` and `tddy-session-split`

## Technical changes

### State B (receivers, checked 2026-09-23)

Checked read-only against every `Cargo.toml`, with host coupling measured at function level. The first
draft of this table placed four topics in blocked receivers. This version replaces it.

| Topic | Production | Receiver | Verdict | Receiver after |
|---|---:|---|---|---:|
| 1 Agent CLI start/resume, **plus 9 stack spawns, `session_toolcall`, and the start/resume/connect parts of `session_coordinate_handlers`** | ~7.5k (~7k after phase 1 dedup) | new `tddy-agent-launch` | viable. It sits above every other receiver and reaches the host through `LaunchHost` | ~7k |
| 3 Agent clones and roster, **plus agent-def resolution** (`svc_resolve_listed_worktree.rs`, 445) | ~3.2k | `tddy-session-agents` (3,601) | needs a port (`AgentHostCallbacks`). New edges: `tddy-daemon-sandbox`, `tddy-projects`, `tddy-semantic-index`, `tddy-stdio`. No cycle | ~6.1k |
| 4 Split and sandboxed-codebase | ~2.2k | new `tddy-session-split` | needs `SplitHost`. It must sit **below** lifecycle, because the facades need lifecycle → receiver. Fallback: merge into agent-launch (~8.9k, no headroom) | ~2.2k |
| 8 Attachments | 919 | `tddy-session-files` (4,606) | viable once the attachment fns leave `svc_resolve_os_user.rs:117-188`. New edge: `tddy-daemon-livekit` | ~5.1k |
| 10 Presenter observation (`presenter_observer_task`, `presenter_intent_client`) | ~220 | `tddy-session-activity` | viable. `PeerRoutedActivity` stays as wiring | — |
| 5a Session catalog, CLI-safe half (`user_sessions_path`, `session_reader`) | ~200 | `tddy-session-catalog` | viable. It adds nothing new to `tddy-coder`'s dependency graph | ~0.95k |
| 5b Session catalog, daemon half (`session_deletion`, `workspace_session`, `session_list_enrichment`, `session_notifications`) | ~1.2k | `tddy-session-activity` | viable. It would pull `tddy-daemon-sandbox`, `-livekit`, `tddy-telegram` and friends into `tddy-coder`, so it **cannot** go to catalog | ~3.0k |
| 6a `pty_runtime`, `tddy_user_config` | ~180 | `tddy-terminal-rpc` | viable, with one new edge to kernel | ~2.6k |
| 6b `task_service`, `action_service` | ~700 | `tddy-daemon-sandbox` (2,531) | viable with **zero new edges**. `tddy-task` and `tddy-actions` both cycle, and `tddy-daemon-rpc` sits above lifecycle | ~3.2k |
| 6c `terminal_session_adapter`, `terminal_bridge_impl` | ~270 | **stay** as wiring | they need `CliSessionManager` and the sandbox stack | — |
| 7 Routing, peers, OS user, admission | ~800 | `tddy-daemon-livekit` (5,333) | viable. **Not `tddy-daemon-kernel`**, which cycles via livekit, host-service, worktree-service and session-files → kernel | ~6.1k |
| 7' `relay_idle`, `local_token_tonic_adapter` | ~120 | `tddy-daemon-kernel` | viable | — |
| 11 Demo VM | 302 | `tddy-demo-runner` (158), or `tddy-vm`, which already has `build_demo_vm_entry` | viable. **Not `tddy-daemon-rpc`**: it depends on lifecycle, so the facade for `DemoVmServiceImpl`/`demo_vm_entry()` (used by `tddy-daemon/src/runtime.rs:282,1046,1141`) is impossible | ~0.5k |
| 2 Host struct, builders, ports, `PeerRouted*`, adapters, `SessionHandler`/`SessionService` impls, `daemon_rpc_handler`, facades | — | **stays** | wiring | see below |

### Phase 2 design

**Cross-topic cycles.** These must be cut before anything moves:

| Cycle | Cut |
|---|---|
| T1 ↔ T3 | T3 calls `resolvable_agent_defs` and `agent_def_for_spawn` (`svc_start_hosted_agent_clone.rs:245,288`, `svc_turn_end_reporter.rs:72`). Move agent-def resolution down into `tddy-session-agents`, so that T1 → T3 only |
| T1 ↔ T4 | `svc_start_session_core.rs:193,198` dispatches into split, and split re-enters `start_session_core`. Add a `SplitHost::start_workspace_session` port |
| T3 → T4 | a single constant getter, `split_forward_deadline` (`svc_provision_agent_clone.rs:99`). Move it to kernel |
| T3 → T2 and T4 → T2 | `worktree_snapshot` (`agent_roster.rs:40`) and `delete_session` (`svc_spawn_split_agent.rs:449`) re-enter the host's handlers. Callback traits |

T5, T6, T7 and T10 are **leaves**: they call no other topic.

**The port pattern is inverted.** #520's `from_host(&DaemonSessionHost)` works because `tddy-daemon-rpc`
sits above lifecycle. Every phase 2 receiver sits **below** it. So each receiver defines a state
struct and a callback trait, and lifecycle builds the struct from its fields and implements the
trait. This is the `SessionFilesPorts`/`SessionAgentPorts` shape `#unbundle` already uses.

| State struct | Fields | Callback trait |
|---|---|---|
| `AgentLaunchState` | T1's 13 fields plus handles to the roster, split and attachment states | `LaunchHost { sandbox_rpc_handler(); pr_stack() }` |
| `AgentRosterState` | `config`, `tddy_data_dir`, `user_resolver`, `peer_routing`, `room_roster`, `session_rooms`, `session_agent_rosters`, `session_agent_clones`, `hosted_agent_clones`, `roster_keepalive_interval` | `AgentHostCallbacks { worktree_snapshot; run_exec_tool_locally; local_exec_tools }` |
| `SplitState` | `config`, `tddy_data_dir`, `session_rooms`, `workspace_sandboxes` | `SplitHost { start_workspace_session; delete_session; session_room_services; agent_tool_socket }` |
| `AttachmentState` | `config`, `tddy_data_dir`, `staging_base_dir`, `peer_routing` | — (calls T7 directly) |
| `AdmissionState`, `OsUserResolver` | `session_admissions`, `session_rooms`, `user_resolver`, `config`, `tddy_data_dir` | — |
| `PresenterObserverDeps` | `tddy_data_dir`, `presenter_event_sink`, `session_notification_bus` | — |
| `DemoVmState` | `vms`, `tddy_data_dir`, `user_resolver`, `rpc_activity` (`DemoVmHandle` leaves `activity_hub.rs`) | — |

**Misplaced code — relocated by #524** so the phase 2 moves carry nothing from another topic:

- `start_split_claude_cli_session` (T4) is in `svc_materialize_staged_attachment.rs:270`.
- The attachment fns (T8) and `run_exec_tool_locally` are in `svc_resolve_os_user.rs:117-188`.
- `session_dir_for` and `ensure_session_room` are in `svc_resolve_listed_worktree.rs:365,385`.
- The jail and subagent env builders are in `svc_turn_end_reporter.rs:176,211`.
- `svc_resolve_tddy_tools_path.rs` holds host builders, plus `record_rpc_activity`:420,
  `mint_first_admission_token`:218 and `maybe_spawn_presenter_observer`:432.

**Move order (leaves first):**
1. `relay_idle` and `local_token` → kernel; demo VM → demo-runner
2. T6 (6a, 6b)
3. T10
4. T7 → daemon-livekit
5. T8
6. T5a and T5b
7. T3 plus agent-defs
8. T4 behind `SplitHost`
9. T1 + T9 + the coordinate handlers → agent-launch

### What "wiring only" can reach

The **wiring floor is ~3.9k**, not ≤1.5k:

| Part | Lines |
|---|---:|
| Ports | ~1.5k (`PeerRouted*` ~0.77k, adapters ~0.45k) |
| Builders | ~0.48k |
| Struct and `mod` decls | ~0.3k |
| `lib.rs`, `handler`, `service`, lifecycle ports, `daemon_rpc_handler` | ~0.66k |
| `test_util` | 0.37k |
| `service_util` | 0.13k |
| Terminal adapter and bridge | 0.27k |

| If also… | Lifecycle ends at |
|---|---:|
| nothing more | ~3.9k |
| `test_util` gated or moved to a testkit, `service_util` moved down | ~3.4k |
| + the `PeerRouted*` wrappers moved down behind a forwarding port (developer's call, see Boundaries) | ~2.6k |
| + adapters as receiver-side impls, builders collapsed | ≤ ~1.5k |

**Wiring crate, defined:** no file ≥ 500 production lines, and no function beyond construction,
delegation and port impls. The size target is **the developer's to choose** from the table above.

## Decisions & trade-offs

- **Two PRs** (developer, 2026-09-23). #524 restructures, this PR extracts.
- **Wiring target ~3.4k** (developer, 2026-09-25): the moves, plus `test_util` gated or moved to a testkit and `service_util` moved down. `PeerRouted*` stays in lifecycle.
- **No shape tests** (developer, 2026-09-25). The plan is the contract.
- **Receivers were chosen by the dependency graph, not by topic name.** Four first-draft placements
  were cycles or layering breaks:
  - routing → kernel is a cycle;
  - demo VM → `tddy-daemon-rpc` makes the facade impossible;
  - the whole catalog topic → `tddy-session-catalog` drags daemon crates into `tddy-coder`;
  - the whole terminals topic → `tddy-terminal-rpc` drags the sandbox stack into `tddy-coder`.
- **`tddy-session-split` sits below lifecycle**, not above it as memory `carve-size-target-10k` first
  proposed. A crate above lifecycle cannot be re-exported by it.
- **The port pattern is inverted relative to #520**, because every receiver is below lifecycle.
- **`PeerRouted*` stays** unless the developer approves the forwarding port.

## Refactoring needed

## Validation results

## TODO

- [x] Phase 2 design checked against the dependency graph
- [x] Create changeset — this document
- [x] USER REVIEW — wiring target ~3.4k, no shape tests (2026-09-25); `PeerRouted*` stays (the ~3.4k row keeps it)
- [x] Rebase onto a green #524
- [ ] Implementation
- [ ] `/validate-changes`
- [ ] `/pr-wrap`
- [ ] Wrap documentation (`/wrap-context-docs`)

## Final Checklist

Executed at wrap:

- [ ] `tddy-session-lifecycle` meets the wiring definition at the chosen size
- [ ] Every receiver ≤ 10k production lines; none depends on `tddy-session-lifecycle`; no file ≥ 500 in any receiver
- [ ] Every public `tddy_session_lifecycle::…` path resolves; consumers unedited apart from listed exceptions
- [ ] Baseline numbers matched
- [ ] `/analyze-code-issues` run on every new crate
