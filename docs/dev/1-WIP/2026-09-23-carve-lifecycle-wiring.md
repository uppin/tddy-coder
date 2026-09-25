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
- **Engine moves only** (developer, 2026-09-25): every move goes through `tddy-tools restructure`; a refusal stops for the developer; hand edits only as post-move build fixes, each cause filed in `docs/dev/todo/`.
- **After the first move run** (developer, 2026-09-25):
  - the "stays behind … names" `check` finding that blocked 1a and 2a is an engine defect. Fix it in `tddy-code-restructuring`, then move. 1a needs no unnamed edges (kernel → `tddy-task`/`tddy-github` are not approved); find a receiver that needs none, or report;
  - move 3 is re-sequenced: T5b moves into `tddy-session-activity` first, T10 follows;
  - 1b (demo VM) is deferred to the port moves (7–9): `DemoVmState` does not exist and the handlers are `impl DaemonSessionHost`;
  - the warm index resolving a nested worktree to the enclosing checkout is fixed first, in its own commit.
- **After the engine fixes** (developer, 2026-09-25):
  - 1a's edges are approved: `tddy-daemon-kernel` gains `tddy-task`, `tddy-github` and `tonic` (neither closes a cycle);
  - the 2a `check --deep` hang ("waiting for type inference at the anchor") is an engine defect, fixed test-first before 2a runs;
  - `check` is tightened to predict `apply`'s cycle refusal for a mutual set split across separate `move_module_to_crate` ops.
- **After moves 1a/2a, T5b re-planned** (developer, 2026-09-25):
  - T5a (`user_sessions_path`, `session_reader`, holding `is_pid_alive`) moves into `tddy-session-catalog` first; `tddy-session-activity` → `tddy-session-catalog` edge approved;
  - lifecycle's `session_notifications` stays as wiring (it collides with the receiver's own module; no engine op merges files);
  - `workspace_session` stays until the host-port moves (it calls private host fns);
  - edges approved for T5b: `tddy-session-files`, `tddy-projects`, `chrono`;
  - `tonic` approved on `tddy-session-activity` for T10.
- **After the third move run** (developer, 2026-09-25):
  - T5a goes to `tddy-session-activity`, not `tddy-session-catalog`: into catalog it would pull kernel, worktree-service and core in (`tddy-bsp` +85 packages), and every caller is daemon-side. It moves as one cluster with `session_deletion`; the edges T5a brings (`tddy-daemon-kernel`, `tddy-worktree-service`, `tddy-core`, `anyhow`, `libc`) come with that choice;
  - T5b's edges approved: `libc`, `tddy-daemon-sandbox`, `tddy-daemon-livekit`; dev `tddy-workflow`, `tempfile`;
  - T10 is deferred to the port moves (7–9): `SessionNotificationPublishing` lives only in lifecycle's staying `session_notifications`.
- **Port moves: hand-written ports, engine-moved bodies** (developer, 2026-09-25, after the fourth move run):
  - each receiver's state struct, callback trait and lifecycle's trait impl may be hand-written: they are new wiring, not moved code;
  - method bodies are never re-typed: they are turned into functions and moved by the engine (`extract_method`, `extract_module`, the move ops); hand edits to them stay post-move build corrections;
  - T3 is piloted first and reported before T4 and T1;
  - `authorize_exec_tool_caller` (`svc_resolve_os_user.rs`) goes with T3 into `tddy-session-agents`.
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

All runs are scoped to the packages named, on macOS (Darwin 25.6). The whole workspace is CI's to report.

### Baseline (HEAD `d9f8f7b3`, 2026-09-25)

`./dev cargo test -p tddy-session-lifecycle --no-fail-fast -- --test-threads=1 --skip sandboxed_bash_pty_action_streams_output`:
**621 passed, 23 failed, 1 ignored**. That is #524's 22 failures by name, plus
`session_room_acceptance::the_first_connect_makes_the_sessions_terminal_drivable_over_livekit`,
which **passes when re-run alone**. It is flaky, not a regression.

`tddy-daemon-sandbox` **does not compile its test targets on HEAD on macOS**:
`tests/sandbox_stdio_seatbelt_acceptance.rs` (`#![cfg(target_os = "macos")]`) has 3 × `E0425`
(`SandboxHandle` not imported). Linux CI never builds it. It stays broken here, since fixing it is
outside this change. Every `tddy-daemon-sandbox` run below excludes that one binary, and its
`--all-targets` check and clippy report only those 3 errors.

### Moves 1–3 (run of 2026-09-25): one engine-applied move, the rest refused or wrong premises

| Move | Plan | Engine result | Outcome |
|---|---|---|---|
| 1a `relay_idle`, `local_token_tonic_adapter` → kernel | (not committed) | `check` gate: 3 findings, `… stays behind … and names relay_idle` | **stopped.** It also needs kernel → `tddy-task` and kernel → `tddy-github`, edges the plan does not name |
| 1b demo VM → `tddy-demo-runner` | (not committed) | `plan is malformed: … still names tddy-session-lifecycle (…activity_hub, …DaemonSessionHost)` | **stopped: wrong premise.** The handlers are `impl DaemonSessionHost`, and `DemoVmState` does not exist yet. `activity_hub.rs` holds *only* `DemoVmHandle`, so there is nothing to split out |
| 2a `pty_runtime`, `tddy_user_config` → `tddy-terminal-rpc` | `02a-…` | `check` gate: 2 findings, `cli_session_manager/pty_handle.rs` and `pty_spawn.rs` name `pty_runtime` | **stopped** |
| 2b `task_service`, `action_service` → `tddy-daemon-sandbox` | `02b-…` | `check --deep` clean; `apply` 4 of 4, then **the tree no longer compiles** | **engine-applied**, then build fixes by hand ([extern-name todo](../todo/2026-09-25-restructure-move-to-crate-leaves-the-destinations-own-extern-name.md), [glob-facade todo](../todo/2026-09-25-restructure-test-binary-move-cannot-see-through-a-glob-facade.md)) |
| 3 T10 → `tddy-session-activity` | (not committed) | `plan is malformed: … presenter_observer_task.rs still names tddy-session-lifecycle (…session_notifications::{)` | **stopped: wrong premise.** T10 is not a leaf; it uses T5b's `SessionNotificationPublishing` |

The warm index daemon **served the main checkout** to this nested worktree
([todo](../todo/2026-09-25-restructure-warm-index-serves-the-main-checkout-for-a-nested-worktree.md)).
Plan 02b's `check --deep`, dry run and apply were all run cold (`TDDY_INDEX_SOCKET` unset). The
`check` gate findings for 1a, 2a and 3 are static, so they read this tree. The `plan is malformed`
refusals quoted for 1b and 3 came from warm runs against the other tree. Cold re-runs against this
tree (after 2b) returned the same refusals, word for word.

**Move 2b in numbers:**

| Measure | Result |
|---|---|
| Production lines (this changeset's counter) | lifecycle 22,939 → 22,237; `tddy-daemon-sandbox` 2,536 → 3,239 |
| New edges | none (zero, as planned); no receiver depends on lifecycle |
| Facade | `pub use tddy_daemon_sandbox::*;` in lifecycle's `lib.rs` |
| Consumers edited | none (`tddy-daemon/src/runtime.rs` resolves through the facade) |
| `cargo check --all-targets` | clean on lifecycle, `tddy-daemon-sandbox`, `tddy-daemon-rpc`, `tddy-daemon`, `tddy-telegram-control`, apart from the pre-existing seatbelt errors |
| Clippy `-D warnings` | clean on lifecycle and `tddy-daemon-sandbox`, apart from the pre-existing seatbelt errors |
| `restructure verify --against HEAD` | 396,995 statements before and after, every one accounted for |
| `tddy-daemon-sandbox` tests (seatbelt binary excluded) | 32 passed, 1 failed, 1 ignored. The failure, `sandbox_session_stdio_acceptance::real_daemon_session_drives_a_seatbelt_jailed_sandbox_runner_entirely_over_stdio` (`tool dispatch timed out`), is **pre-existing**: it fails identically with HEAD's `lib.rs`. `sandboxed_bash_pty_action_streams_output` is skipped here as in lifecycle's baseline, since it moved with its binary |
| Lifecycle tests | **615 passed, 22 failed, 1 ignored**, with zero new failures by name. The 22 are #524's 22; the flaky session-room test passed. 622 − 7 = 615: the 7 tests that moved with `action_service_acceptance` (2) and `action_sandbox_acceptance` (5 run, 1 skipped) pass in `tddy-daemon-sandbox` |

`task_service_acceptance.rs` **stays in lifecycle**. Three of its tests drive
`claude_cli_session::ClaudeCliSessionManager` (lifecycle's `CliSessionManager`, T6c, which stays),
so moving it would make the receiver dev-depend on lifecycle.

### Second move run (2026-09-25, after the engine fixes)

Every restructure command ran against this worktree's own warm index daemon, restarted on a binary
built after `5446cec6` and `ebeb8282` (the one that was running predated them). Line counts here come
from a re-implementation of the discovery doc's definition, which reads HEAD's lifecycle as 22,116
rather than 22,237. The before and after figures use that one counter throughout.

**Receiver baselines on HEAD `ebeb8282`:** `tddy-daemon-kernel` 122 passed; `tddy-terminal-rpc` 55
passed; `tddy-session-activity` has no tests. Lifecycle: 615 passed, 22 failed, 1 ignored, with the
same 22 by name.

#### Move 1a: `relay_idle`, `local_token_tonic_adapter` → `tddy-daemon-kernel`

| Measure | Result |
|---|---|
| Plan | `01a-relay-idle-local-token-to-kernel.jsonl`, two `move_module_to_crate` with `reexport: glob` |
| Engine | plain `check`: no findings. `check --deep`: no findings (6m27s, cold index). Dry run: 2 of 2 resolved. `apply`: 2 of 2 applied, then **the tree no longer compiles** (`tddy-daemon-kernel` depends on itself) |
| Hand fixes | `use tddy_daemon_kernel::config::DaemonConfig;` → `use crate::config::DaemonConfig;` in the moved adapter, and the kernel's `tddy-daemon-kernel = { path = "" }` self-edge removed ([new cause: facade followed back to the destination](../todo/2026-09-25-restructure-move-to-crate-follows-a-facade-back-to-the-destination.md)). The second, identical `pub use tddy_daemon_kernel::*;` in lifecycle's `lib.rs` removed (cause already recorded) |
| Facade | `pub use tddy_daemon_kernel::*;` in lifecycle's `lib.rs`. `tddy_session_lifecycle::relay_idle::…` and `…::local_token_tonic_adapter::…` resolve through it |
| New edges | kernel → `tddy-task`, `tddy-github`, `tonic` (approved). No receiver depends on lifecycle |
| Consumers edited | none |
| Production lines | lifecycle 22,116 → 21,998; kernel 3,290 → 3,409 |
| `restructure verify --against HEAD` | 397,648 statements before and after, every one accounted for |
| `cargo check --all-targets` | clean on kernel, lifecycle, `tddy-daemon-rpc`, `tddy-daemon`, `tddy-telegram-control` |
| Clippy `-D warnings`, `cargo fmt` | clean on kernel and lifecycle |
| Tests | kernel 122 passed; lifecycle 615 passed, 22 failed, 1 ignored, the same 22 by name. Neither file had tests of its own, and no lifecycle integration test exercises them, so none moved |

#### Move 2a: `pty_runtime`, `tddy_user_config` → `tddy-terminal-rpc`

| Measure | Result |
|---|---|
| Plan | `02a-pty-runtime-to-terminal-rpc.jsonl`, one `move_cluster_to_crate` with `reexport: glob` |
| Engine | plain `check`: no findings. `check --deep`: no findings (1.5s, warm). It reported `pty_runtime.rs:159` (`#[cfg(not(unix))] fn resolve_final_argv_env`) as inactive, so only the unix cfg was surveyed. Dry run: 1 of 1 resolved. `apply`: 1 of 1 applied, then **the tree no longer compiles** (`tddy_user_config`'s inline tests: 2 × `E0433`, `tempfile` unlinked) |
| Hand fixes | `tempfile = "3"` added to terminal-rpc's `[dev-dependencies]` ([new cause: a crate named only in a body path](../todo/2026-09-25-restructure-move-to-crate-misses-a-crate-named-only-in-a-body-path.md)). The engine's two `pub use tddy_terminal_rpc::*;` lines became one `pub use tddy_terminal_rpc::{pty_runtime, tddy_user_config};`: the root glob re-exports terminal-rpc's `service`, which lifecycle's private `mod service;` shadows (`hidden_glob_reexports`, so clippy fails) ([new cause: a root glob facade](../todo/2026-09-25-restructure-glob-facade-re-exports-a-name-the-origin-shadows.md)) |
| Facade | `pub use tddy_terminal_rpc::{pty_runtime, tddy_user_config};` in lifecycle's `lib.rs` |
| New edges | terminal-rpc → `tddy-daemon-kernel` (named in State B). No cycle: the kernel does not reach terminal-rpc. It adds nothing to any binary's graph, because `tddy-coder`, `tddy-sandbox-app` and `tddy-tools --no-default-features` already carry the kernel, `livekit`, `tddy-github` and `tddy-task`. Dev: `tempfile` (already in `Cargo.lock`) |
| Consumers edited | none |
| Non-unix read-through | `resolve_final_argv_env`'s `cfg(not(unix))` body names only `PtySpawnSpec` and `ResolvedArgvEnv`, both unconditional items of the same moved file, so it resolves unchanged. No fix was needed. **Pre-existing, not caused by the move:** the file's unconditional `pub use tddy_daemon_kernel::privilege_drop::{…, resolve_pty_os_user, ResolvedPtyUser}` names two `#[cfg(unix)]` kernel items, so neither lifecycle on HEAD nor terminal-rpc now builds on non-unix. The line moved byte for byte |
| Production lines | lifecycle 21,998 → 21,821; terminal-rpc 2,348 → 2,526 |
| `restructure verify --against HEAD` | 397,648 statements before and after, every one accounted for |
| `cargo check --all-targets` | clean on terminal-rpc, lifecycle, `tddy-daemon-rpc`, `tddy-daemon`, `tddy-telegram-control` |
| Clippy `-D warnings`, `cargo fmt` | clean on terminal-rpc and lifecycle |
| Tests | terminal-rpc 63 passed (55 + the 8 inline tests that moved: 6 in `pty_runtime`, 2 in `tddy_user_config`); lifecycle 607 passed, 22 failed, 1 ignored (615 − 8), the same 22 by name |

#### Move 3 (T5b): session catalog, daemon half → `tddy-session-activity`: **stopped, wrong premise**

Plan `03a-session-catalog-daemon-half-to-session-activity.jsonl`. It has four `move_module_to_crate`
ops, not a cluster, because the four modules reference each other one way only
(`session_notifications` → `session_list_enrichment`, which is ordered first). The plain `check`
gate refused it:

```text
2: `packages/tddy-session-lifecycle/src/session_deletion.rs`, which operation 2 moves to `packages/tddy-session-activity`, names `tddy_session_lifecycle::session_reader::is_pid_alive`, which stays behind in `packages/tddy-session-lifecycle`. So the destination would depend on the crate it left, while the facade it leaves there names the destination: a cycle `apply` refuses. Move them in one `move_cluster_to_crate`, with this module as its anchor and what those paths reach in `also`, or leave `session_deletion` where it is
```

T5b is **not a leaf**, and none of the four modules can move on its own terms:

| Module | Blocker | Kind |
|---|---|---|
| `session_deletion` | `use crate::session_reader::is_pid_alive;`. `session_reader` is T5a, bound for `tddy-session-catalog`. Taking it along moves T5a into the wrong receiver, and sending it to catalog first adds an activity → catalog edge the changeset does not name | cycle (engine finding) |
| `workspace_session` | body paths `crate::connection_service::{find_registered_project, project_repo_root, starting_session_metadata}`: the host's `service_util`, all three `pub(crate)` | cycle and private path. **`check --deep` did not see it** ([todo](../todo/2026-09-25-restructure-check-misses-a-body-path-to-a-module-staying-behind.md)) |
| `session_notifications` | `tddy-session-activity` **already has** a `session_notifications` module (403 lines, `#unbundle` node 7's half), and lifecycle's 96-line half globs it back in. Moving it is a merge, which no operation performs | name collision. **`check --deep` did not see it** ([todo](../todo/2026-09-25-restructure-check-misses-a-module-name-the-destination-already-has.md)) |
| `session_list_enrichment` | movable in isolation, but it needs edges the changeset does not name for T5b: `tddy-session-files` (`session_context_docs`; its tests use `session_attachments`), and **`chrono`**, a new external dependency for activity. Its tests add `tddy-workflow` and `tempfile` | unnamed edges |

The other edges T5b needs are named or existing: `tddy-daemon-sandbox`, `tddy-daemon-livekit` and
`tddy-telegram` (checked: none reaches `tddy-session-activity`), `tddy-worktree-service`, and
`tddy-projects` (`project_storage`), which is not named either. A diagnostic `check --deep` without
`session_deletion` reported `no findings`. Nothing was applied.

**What it would take** (the developer's call): `service_util` moved down first (it is already part
of the ~3.4k target), which unblocks `workspace_session`; T5a into catalog first plus an approved
activity → catalog edge, or `is_pid_alive` moved to the kernel; lifecycle's `session_notifications`
half kept as wiring or merged by hand (a manual write, which the engine-only rule forbids); and the
edges to `tddy-session-files`, `tddy-projects` and `chrono` approved.

#### Move 4 (T10): presenter observation → `tddy-session-activity`: **not run, depends on T5b**

`presenter_observer_task` imports `crate::session_notifications::{notification_for_presenter_event,
SessionNotificationPublishing}`. The first is activity's own, re-exported through lifecycle's glob.
The second is defined in lifecycle's half and stays behind with T5b. The static `check` of the existing
`03-…` plan reports it as `names tddy_session_lifecycle::session_notifications::{, which stays
behind`. The `::{` quote is cosmetic: a grouped `use` printed unexpanded. The finding itself is
correct, and it is the blocker. T10 also needs **`tonic`** on `tddy-session-activity` (both files
use `tonic::transport::Endpoint`), which is not approved. `03b-…` was not written and `03-…` was
kept.

### Third move run (2026-09-25, T5a → T5b → T10): every move stopped before `apply`

Every restructure command ran against this worktree's own warm index daemon (pid 72639). Its binary
and `tddy-tools` post-date every engine commit. **Nothing was applied.** No package file changed, and
the only commit is the plans and these findings. Line counts use the previous run's counter, which
reads HEAD's lifecycle as 21,821, the same figure that run ended on.

**Baselines on HEAD `261fd28a`:**
- Lifecycle: **607 passed, 22 failed, 1 ignored**, the same 22 by name. The failures are 5 in
  `sandbox_behavior_acceptance`, 5 in `sandboxed_claude_cli_acceptance`, 4 in
  `sandboxed_cursor_cli_acceptance`, 2 in `sandboxed_session_lifecycle_acceptance` and 6 in
  `session_sync_livekit_acceptance`. The flaky session-room test passed.
- `tddy-session-catalog`: 18 passed (5 unit, 1 `session_catalog_acceptance`, 12
  `session_catalog_red`).
- `tddy-session-activity`: no tests.
- `tddy-coder`'s catalog tests were not run, because no catalog path changed.

Since nothing was applied, there was no after-run, no post-move check or clippy, and no `verify`.

| Move | Plan | Engine result | Outcome |
|---|---|---|---|
| T5a `user_sessions_path`, `session_reader` → `tddy-session-catalog` | `03a-session-catalog-cli-half-to-session-catalog.jsonl` (two `move_module_to_crate`, `reexport: glob`) | plain `check`: no findings. `check --deep`: no findings (3s, warm). Dry run: 2 of 2 resolved (6 and 5 files) | **stopped: new catalog edges.** See below |
| T5b `session_list_enrichment`, `session_deletion` → `tddy-session-activity` | `03b-session-catalog-daemon-half-to-session-activity.jsonl` (two `move_module_to_crate`, `reexport: glob`) | plain `check`: 1 finding. `check --deep`: op 0 clean, op 1 refused (quoted below) | **stopped.** `session_deletion` depends on T5a; both modules need edges nobody approved |
| T10 `presenter_observer_task`, `presenter_intent_client` → `tddy-session-activity` | `03c-presenter-to-session-activity.jsonl` (one `move_cluster_to_crate`, `reexport: glob`; replaces the untracked `03-…`) | plain `check`: 1 finding. `check --deep`: refused (quoted below) | **stopped: it still names a module that stays behind** |

#### T5a: the premise holds for `tddy-coder`, but catalog gains five edges

Moving the two modules gives `tddy-session-catalog` five direct dependencies it does not have today:

| New catalog edge | Needed by |
|---|---|
| `tddy-daemon-kernel` | `user_sessions_path` re-exports `tddy_daemon_kernel::user_paths::{home_dir_for_user, …}` |
| `tddy-worktree-service` | `session_reader::DaemonSessionListing` implements `tddy_worktree_service::branch_owner::SessionListing` |
| `tddy-core` | `session_reader` reads `SessionMetadata` and `SESSIONS_SUBDIR` |
| `anyhow` | `list_sessions_in_dir` returns `anyhow::Result` |
| `libc` | `username_for_uid` (`getpwuid_r`) and `is_pid_alive` (`kill`) |

The rule for this move was **any new catalog edge: stop and report**, so nothing was applied. What
the edges would do to each graph, measured with `cargo tree -e normal` as the union of the kernel's
and the worktree service's trees against each consumer's graph on HEAD:

| Graph | Packages on HEAD | Packages T5a would add |
|---|---:|---:|
| `tddy-coder` | 452 | **0**: it already carries all five |
| `tddy-tools` (default and `--no-default-features`) | 462 | **0** |
| `tddy-bsp` (catalog's other normal consumer) | 343 | **85**, among them `tddy-daemon-kernel`, `tddy-worktree-service`, `tddy-livekit`, `tddy-sandbox`, `tddy-github`, `tddy-projects`, `tddy-discovery`, `tddy-credentials` and `tddy-session-tool-client` |

So State B's claim is true for every binary: `tddy-bsp` is reached only through `tddy-coder`,
`tddy-tools`, `tddy-daemon` and lifecycle, which all carry these crates already. It is false for
`tddy-bsp` built on its own. No cycle: nothing that `tddy-daemon-kernel`, `tddy-worktree-service` or
`tddy-core` depends on reaches catalog.

**Hand fixes an apply would need.** Neither has a todo, because nothing was applied.
- `session_reader::is_pid_alive` is `pub(crate)`, and `session_deletion`, which stays, calls it.
  The fix is `pub` visibility.
- A root `pub use tddy_session_catalog::*;` would also re-export catalog's `entry`, `error`,
  `populate`, `provider`, `read` and `store` beside lifecycle's two other root globs. The fix is a
  named `pub use tddy_session_catalog::{session_reader, user_sessions_path};`, the cause already
  filed as the glob-facade todo.

The survey found no consumer that a facade leaves unserved: 17 callers across `tddy-daemon`,
`tddy-daemon-rpc`, `tddy-telegram-control`, `tddy-worktree-service`'s tests and lifecycle itself.

Production lines if applied: lifecycle 21,821 → 21,641 (−180); catalog 747 → 927.

#### T5b: `session_deletion` waits on T5a, and both modules need unapproved edges

The engine refused op 1 (`session_deletion`), statically and deep:

```text
1: plan is malformed: `packages/tddy-session-lifecycle/src/session_deletion.rs` still names `tddy-session-lifecycle` (tddy_session_lifecycle::session_reader::is_pid_alive), so the destination would depend on the crate it left while that crate goes on naming the module it lost — move what those paths reach, or move the module's own dependencies with it
```

The engine classes it `plan is malformed`, but the plan is not the problem. The only plan-side fix
is to take `session_reader` along, which moves T5a into the wrong receiver. The real remedy is T5a
first. `session_list_enrichment` (op 0) is clean under `check --deep` and could move on its own. Both
modules still need edges outside the approved four (`tddy-session-catalog`, `tddy-session-files`,
`tddy-projects`, `chrono`):

| Module | Edges it needs on activity | Approved? |
|---|---|---|
| `session_list_enrichment` | `tddy-session-files` (`session_context_docs`), `chrono` | yes |
| | `tddy-telegram`, `tddy-core`, `tddy-service`, `anyhow`, `log`, `serde_json` | already activity's |
| | **dev:** `tddy-workflow` (`session_attachments_root`, test line 1428), `tempfile` (test lines 330, 1429) | **no** |
| `session_deletion` | `tddy-projects` (`project_storage`), `tddy-session-catalog` (after T5a) | yes |
| | `tddy-daemon-sandbox` (`RUNNER_PID_FILE`), `tddy-daemon-livekit` (`SessionRoomRegistry`) | named in State B's 5b row, not in the decision |
| | **`libc`** (`kill` in `signal_pid` and `teardown_workspace_sandbox`), dev `tempfile` | **no** |

No cycle: only lifecycle, `tddy-daemon-rpc`, `tddy-daemon`, `tddy-desktop` and
`tddy-telegram-control` depend on activity (`cargo tree -i -e normal`).

Production lines if both moved: lifecycle −782 (`session_deletion` 459, `session_list_enrichment`
323); activity 1,573 → 2,355.

#### T10: `SessionNotificationPublishing` still stays behind

```text
0: plan is malformed: `packages/tddy-session-lifecycle/src/presenter_observer_task.rs` still names `tddy-session-lifecycle` (tddy_session_lifecycle::session_notifications::{), so the destination would depend on the crate it left while that crate goes on naming the module it lost — move what those paths reach, or move the module's own dependencies with it
```

`presenter_observer_task.rs:16` still imports
`crate::session_notifications::{notification_for_presenter_event, SessionNotificationPublishing}`.
`SessionNotificationPublishing` is defined only in lifecycle's `session_notifications.rs` (the
96-line half that now stays as wiring), and is used as a parameter type in
`spawn_presenter_observer_task`. **Activity's `session_notifications` defines no equivalent.** It
holds the bus, the event, the subscriber trait and the builders (`notification_for_presenter_event`
among them), and names `SessionNotificationPublishing` only in its module doc (`:28`). T10 cannot
move while that type stays behind. Per the instruction, nothing was redesigned around it.
`presenter_intent_client` names nothing in lifecycle, but it is only half of the cluster.

### Fourth move run (2026-09-25: T5a + T5b, then T7, then T8)

Every restructure command ran against this worktree's warm index daemon (pid 72639). Line counts use
the same counter as the previous runs, which reads HEAD's lifecycle as 21,821.

`target/` was deleted during the run (to free disk). The half-finished baseline was discarded. After
the deletion finished, `./test`'s prebuild set and `tddy-index-daemon` were rebuilt, and the baseline
was re-run from scratch.

**Baselines on HEAD `8f0d9302`:**
- Lifecycle: **607 passed, 22 failed, 1 ignored**, the same 22 by name (5 `sandbox_behavior_acceptance`,
  5 `sandboxed_claude_cli_acceptance`, 4 `sandboxed_cursor_cli_acceptance`, 2
  `sandboxed_session_lifecycle_acceptance`, 6 `session_sync_livekit_acceptance`). The flaky
  session-room test passed.
- `tddy-session-activity`: no tests.
- `tddy-daemon-livekit`: 174 passed.
- `tddy-session-files`: 160 passed.

#### Move 1 (T5a + T5b): session catalog → `tddy-session-activity`

| Measure | Result |
|---|---|
| Plan | `03b-session-catalog-daemon-half-to-session-activity.jsonl`, reworked. One `move_cluster_to_crate` (`session_deletion`, `session_reader`, `user_sessions_path`, `session_list_enrichment`; `reexport: glob`), then `move_test_binary_to_crate` for `tests/worktree_removal_eligibility.rs`. `03a-…` (T5a → catalog) was **deleted**, because the "After the third move run" decision makes it wrong |
| Blind-spot read before apply | No body path reaches a module that stays behind. `session_list_enrichment`'s body paths `crate::elicitation::…` and `crate::session_context_docs::…` are facades over `tddy-telegram` and `tddy-session-files`. No module name collides: activity has `service`, `session_notification_subscribers`, `session_notifications` and `streams` |
| Kernel callers | `tddy-daemon-kernel/src/user_paths.rs` and `lib.rs` mention `user_sessions_path` in comments only. The kernel's own module is `user_paths`, so nothing collides and there is no kernel → activity need |
| Engine | plain `check`: no findings. `check --deep`: no findings (2.4s, warm). Dry run: 2 of 2 resolved (11 and 3 files). `apply`: 2 of 2 applied, then **the tree no longer compiles** (18 errors in activity's lib) |
| What the engine did right | re-pointed the top-level `use crate::project_storage`/`worktrees`/`session_context_docs` lines to `tddy_projects`, `tddy_worktree_service` and `tddy_session_files`; kept `crate::session_reader::is_pid_alive` for the co-moving module; added `chrono`, `tddy-projects` and `tddy-session-files` to activity |
| Hand fixes | **Manifest:** activity gains `tddy-daemon-sandbox`, `tddy-daemon-livekit` (named only in body paths: [existing cause](../todo/2026-09-25-restructure-move-to-crate-misses-a-crate-named-only-in-a-body-path.md)), `libc` under `[target.'cfg(unix)'.dependencies]` as in lifecycle (registry crate, documented limitation), and dev `tddy-workflow` and `tempfile`. The engine's dev edge `tddy-session-lifecycle` was removed ([existing cause](../todo/2026-09-25-restructure-test-binary-move-cannot-see-through-a-glob-facade.md)). **Qualification:** `crate::elicitation::` → `tddy_telegram::elicitation::` (×2), and `crate::session_context_docs::` → `tddy_session_files::session_context_docs::` in bodies (documented limitation). **`use` lines:** the two `use crate::…` inside `session_list_enrichment`'s `mod tests` → `tddy_session_files::…` ([**new cause**](../todo/2026-09-25-restructure-move-to-crate-skips-the-use-lines-of-the-moved-files-test-module.md)), and the moved test binary's `use tddy_session_lifecycle::session_deletion::…` → `tddy_session_activity::…`. **Visibility:** `session_deletion::signal_pid` `pub(crate)` → `pub`, since lifecycle's `cli_session_manager/terminals.rs` calls it ([existing cause](../todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md), item 3). `is_pid_alive` needed no widening: both its callers moved. **Facade:** the four identical `pub use tddy_session_activity::*;` became one named `pub use tddy_session_activity::{session_deletion, session_list_enrichment, session_reader, user_sessions_path};` ([existing cause](../todo/2026-09-25-restructure-glob-facade-re-exports-a-name-the-origin-shadows.md)) |
| Facade | the named `pub use` above, in lifecycle's `lib.rs` |
| New edges | activity → `tddy-session-files`, `tddy-projects`, `chrono`, `libc`, `tddy-daemon-sandbox`, `tddy-daemon-livekit`; dev `tddy-workflow`, `tempfile`. All approved; `tddy-session-catalog` was not needed. `tddy-daemon-kernel`, `tddy-worktree-service`, `tddy-core` and `anyhow` were already activity's. No cycle: `cargo tree -i tddy-session-activity -e normal` lists only lifecycle, `tddy-daemon-rpc`, `tddy-daemon`, `tddy-desktop` and `tddy-telegram-control`. Activity has no path to lifecycle, normal or dev |
| Consumers edited | none. No test reads lifecycle source by path for these files (`tddy-daemon/tests/test_placement.rs` and `unbundle_endpoint.rs` name `user_sessions_path.rs` under **`tddy-daemon/src`**) |
| Production lines | lifecycle 21,821 → **20,861**; activity 1,573 → 2,541 |
| `restructure verify --against HEAD` | 397,648 → 397,654 statements. The 5 lost and 11 gained are exactly the hand fixes above (3 qualifications, 1 widening, the named facade and its comment) and fmt's re-wrapping of the lengthened calls |
| `cargo check --all-targets` | clean on lifecycle, activity, `tddy-daemon-rpc`, `tddy-daemon`, `tddy-telegram-control`, `tddy-worktree-service` (its `branch_owner_unit` test names `tddy_session_lifecycle::session_reader`) and `tddy-daemon-kernel` |
| Clippy `-D warnings`, `cargo fmt` | clean on activity and lifecycle |
| Tests | activity **45 passed** (40 inline: 12 `session_deletion`, 27 `session_list_enrichment`, 1 `session_reader`; plus 5 `worktree_removal_eligibility`); lifecycle **562 passed, 22 failed, 1 ignored** (607 − 45), the same 22 by name. `acceptance_daemon.rs` stays in lifecycle: it mostly tests the kernel's `DaemonConfig` and reaches `session_reader` through the facade |

#### Move 2 (T7): routing and admission → `tddy-daemon-livekit`: two modules moved, the rest is a wrong premise

The ~800 lines State B counts for T7 are three files. Only two can move:

| File | Production | Verdict |
|---|---:|---|
| `peer_routing.rs` | 228 | **moved**: free of the host (`PeerRouting` is a value the host holds) |
| `session_admission_service.rs` | 227 | **moved**: free of the host |
| `connection_service/svc_resolve_os_user.rs` | 231 (+150 in two submodules) | **stays, and mixes topics.** See below |

`svc_resolve_os_user.rs` holds four things:

| Items | Lines | Topic | Why it cannot go to livekit as it stands |
|---|---|---|---|
| `impl DaemonSessionHost { resolve_os_user, eligible_instance_ids, classify_*_route, resolve_exec_tool_worktree, context_globs_for_session, common_room_slot, rpc_served_by_peer, stream_served_by_peer }` | 28–143 | host wiring (one-line delegations to `PeerRouting` and the free fns) | an inherent `impl` of lifecycle's type cannot leave lifecycle (`E0116`). The `AdmissionState`/`OsUserResolver` state struct that State B assumes **does not exist**. Activity already exports an unrelated `OsUserResolver` type alias, so that name would collide |
| `resolve_os_user`, `authorize_exec_tool_caller` (free fns) | 146–212 | OS user, exec-tool caller authentication | movable only after an `extract_module` (`to_file: true`) grouping these two whole items. Then a second plan's `move_module_to_crate` on the nested module. **Not done:** `authorize_exec_tool_caller` is as much T3's exec-tool code as T7's, and its partner stays behind (next row) |
| `resolve_exec_tool_worktree` (free fn) | 214–231 | exec tool | its body calls `workspace_session::resolve_worktree_root_for_session`, and `workspace_session` stays in lifecycle, so it can move nowhere below lifecycle |
| `mod local_exec_tool_dispatch` (25) and `mod session_attachment_materialization` (125) | submodules | T3 (`run_exec_tool_locally`) and T8 | both are `impl DaemonSessionHost` blocks. The misplacement the plan names is real: the file carries T3's and T8's code |

So the split the file needs first is: `extract_module` for `resolve_os_user` and `authorize_exec_tool_caller` (a pure grouping), plus the two submodules re-parented under their own topics. That second step is not a grouping, so it is out of scope. Whether the free-fn pair goes to livekit at all is the developer's call.

| Measure | Result |
|---|---|
| Plan | `04a-routing-admission-to-daemon-livekit.jsonl`: one `move_cluster_to_crate` (`peer_routing`, `session_admission_service`; `reexport: glob`). The two do not reference each other; a cluster gives them one facade line |
| Blind-spot read before apply | the body paths are `crate::livekit_peer_discovery::…` (×4, the destination's own module, which resolves once moved) and `crate::remote_git_service::UserResolver` (×2, a facade over `tddy-worktree-service`, which livekit already has). None reaches a module that stays behind. No name collides with livekit's `common_room_supervisor`, `livekit_peer_discovery`, `livekit_rooms_stream`, `livekit_service` or `session_room`. No inline tests, and no lifecycle test binary exercises either module |
| Engine | plain `check`: no findings. `check --deep`: no findings. Dry run: 1 of 1 resolved (7 files). `apply`: 1 of 1 applied, then **the tree no longer compiles** (`tddy-daemon-livekit` depends on itself) |
| Hand fixes | `tddy-daemon-livekit = { path = "" }` removed from its own manifest, and `use tddy_daemon_livekit::{livekit_peer_discovery, session_room}::…` → `use crate::…` (×3) ([existing](../todo/2026-09-25-restructure-move-to-crate-follows-a-facade-back-to-the-destination.md) [causes](../todo/2026-09-25-restructure-move-to-crate-leaves-the-destinations-own-extern-name.md)). `crate::remote_git_service::UserResolver` → `tddy_worktree_service::remote_git_service::UserResolver` (×2, body path, documented limitation). `PeerRouting::{set_eligible_daemon_source, eligible_instance_ids, classify_daemon_route, stream_served_by_peer}` `pub(crate)` → `pub`: lifecycle's host delegations call them (`E0624` ×4, [existing cause](../todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md) item 3). The two root globs became one named `pub use` ([existing cause](../todo/2026-09-25-restructure-glob-facade-re-exports-a-name-the-origin-shadows.md)). **No new cause, so no new todo** |
| Facade | `pub use tddy_daemon_livekit::{peer_routing, session_admission_service};` in lifecycle's `lib.rs`. `tddy-daemon-rpc`'s `tddy_session_lifecycle::peer_routing::PeerRouting` and `tddy-daemon`'s `…::session_admission_service::SessionAdmissionServiceImpl` resolve through it |
| New edges | **none**. Livekit already had `tddy-daemon-kernel`, `tddy-host-service` (`multi_host`), `tddy-worktree-service`, `tddy-livekit`, `tddy-rpc` and `tddy-service`. Livekit has no path to lifecycle, normal or dev |
| Consumers edited | none |
| Production lines | lifecycle 20,861 → **20,406**; livekit 5,409 → 5,863 |
| `restructure verify --against HEAD` (`15374089`) | 397,654 → 397,652 statements. The 10 lost and 8 gained are the 4 widenings, the 2 qualifications and the facade comment, plus fmt joining `classify_daemon_route`'s signature onto one line |
| `cargo check --all-targets` | clean on livekit, lifecycle, `tddy-daemon-rpc`, `tddy-daemon`, `tddy-telegram-control` |
| Clippy `-D warnings`, `cargo fmt` | clean on livekit and lifecycle |
| Tests | livekit **174 passed** (as on HEAD; nothing moved with the code); lifecycle **562 passed, 22 failed, 1 ignored**, the same 22 by name |

#### Move 3 (T8): attachments → `tddy-session-files`: the progress types moved, the rest is a wrong premise

The ~919 lines State B counts for T8 are these:

| File | Production | Verdict |
|---|---:|---|
| `connection_service/attachment_progress.rs` | 115 | **moved**: `AttachmentProgressSink`, `AttachmentProgressReporter`, `AttachmentMaterialization`, `cleanup_materialized_attachments`, `attachment_size_bytes`. All free of the host |
| `connection_service/svc_materialize_staged_attachment.rs` | 245 | **stays**: one `impl DaemonSessionHost` that reads `self.config`, `self.tddy_data_dir`, `self.staging_base_dir`, `self.classify_daemon_route` and `self.common_room_slot`. **It still declares `mod split_claude_cli_start;` (178 lines, T4's `start_split_claude_cli_session`)**, so the misplacement the plan names is still here |
| `connection_service/svc_resolve_os_user/session_attachment_materialization.rs` | 125 | **stays**: one `impl DaemonSessionHost` (`materialize_session_attachments`), and a child of T7's mixed file (see move 2) |
| `svc_session_files_ports.rs` + `svc_peer_routed_session_files.rs` | ~560 | **stays**: ports and `PeerRouted*`, wiring by the Boundaries |

The `AttachmentState` port State B assumes (`config`, `tddy_data_dir`, `staging_base_dir`,
`peer_routing`) **does not exist**, so neither `impl DaemonSessionHost` block can leave lifecycle.
That is the same wrong premise as T7's `AdmissionState` and 1b's `DemoVmState`. Moving
`split_claude_cli_start` out from under the attachment file is a re-parenting, not a grouping, and is
left for the T4 move. The `tddy-daemon-livekit` edge State B names for T8 is needed only by the
host-impl half (`PeerRoute`, `local_instance_id_for_config`). It was not added.

| Measure | Result |
|---|---|
| Plan | `05a-attachment-progress-to-session-files.jsonl`: one `move_module_to_crate` (`reexport: glob`) of the nested `connection_service::attachment_progress` |
| Blind-spot read before apply | body paths `tddy_workflow::session_attachments_root`, `log::warn!` and `tokio::sync::mpsc`, all crates session-files already has. No path reaches a module that stays behind. No name collides with session-files' eleven modules. No inline tests, and no test binary exercises it on its own |
| Engine | plain `check`: no findings. `check --deep`: no findings, survey `5 item(s) reached from outside, 0 caller(s)`. Dry run: 1 of 1 resolved (4 files). `apply`: 1 of 1 applied, then **the tree no longer compiles** (19 errors in lifecycle: 17 × `E0432` in ten `connection_service` children, plus 2 `E0308` cascades) |
| Hand fixes | In `connection_service.rs`, the engine's `pub use tddy_session_files::*;` and the dangling `pub(crate) use attachment_progress::*;` became one `pub(crate) use tddy_session_files::attachment_progress::*;` ([**new cause**](../todo/2026-09-25-restructure-move-to-crate-leaves-a-nested-modules-parent-glob-dangling.md)). Every `pub(crate)` in the moved file → `pub` (21 statements: 3 structs, 11 fields, 5 methods, 2 fns), since lifecycle uses all of them ([existing cause](../todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md), item 3). **Left as-is:** two rustdoc links in the moved file (`crate::livekit_peer_discovery::PEER_FORWARD_STREAM_IDLE_TIMEOUT`, `DaemonSessionHost::start_session_core`) now name items session-files cannot reach. That is a `cargo doc` warning, not a build failure, and CI runs no rustdoc gate |
| Facade | `pub(crate) use tddy_session_files::attachment_progress::*;` in `connection_service.rs`. The module was `pub(crate)`-re-exported before, so no public `tddy_session_lifecycle::…` path existed or was lost |
| New edges | **none** |
| Consumers edited | none |
| Production lines | lifecycle 20,406 → **20,290**; session-files 4,600 → 4,716 |
| `restructure verify --against HEAD` (`96531fcc`) | 397,652 statements before and after; the 21 lost and 21 gained are the 21 widenings |
| `cargo check --all-targets` | clean on session-files, lifecycle, `tddy-daemon-rpc`, `tddy-daemon`, `tddy-telegram-control` |
| Clippy `-D warnings`, `cargo fmt` | clean on session-files and lifecycle |
| Tests | session-files **160 passed** (as on HEAD); lifecycle **562 passed, 22 failed, 1 ignored**, the same 22 by name |

#### Where the fourth run leaves lifecycle

| Crate | HEAD `8f0d9302` | After |
|---|---:|---:|
| `tddy-session-lifecycle` | 21,821 | **20,290** (−1,531) |
| `tddy-session-activity` | 1,573 | 2,541 |
| `tddy-daemon-livekit` | 5,409 | 5,863 |
| `tddy-session-files` | 4,600 | 4,716 |

Every receiver is under 10k, and none depends on lifecycle, normal or dev. What T7 and T8 left
behind is `impl DaemonSessionHost` code. Like 1b and T10, it needs the host-port moves (7–9), whose
state structs do not exist yet.

### Port-move pilot (T3), 2026-09-25: two engine moves, the state-struct route refused by the engine

The pilot followed "Port moves: hand-written ports, engine-moved bodies". Every restructure command ran
against this worktree's warm index daemon, which was restarted on the binaries rebuilt at 15:16. It
had to be restarted twice more, after it hung (see step 1). Line counts use the previous runs'
counter, which reads HEAD `f692add4`'s lifecycle as 20,290.

**Baselines on HEAD `f692add4`:**
- Lifecycle: **562 passed, 22 failed, 1 ignored**, the same 22 by name. The flaky session-room test
  passed.
- `tddy-session-agents`: 72 passed.
- `tddy-daemon-kernel`: 122 passed.

#### The recipe that works: extract the method's `self`-free tail

rust-analyzer's "Extract into function" writes a **free** function when the range holds no `self`,
and a `&self` method inside the same `impl` when it does. A method in `impl DaemonSessionHost` can
never leave lifecycle (`E0116`), so only a `self`-free range can move. Where every `self` read sits
in the method's opening statements, the tail is extracted: the reads stay in the host as the
delegation, and what they produced becomes the parameters. The chain per method is three plans:
1. `extract_method` over the tail;
2. `extract_module` with `to_file: true` over the new function;
3. `move_module_to_crate`.

They are separate plans because each one's anchor exists only after the one before.

#### Step 1: `refuse_unready_clone` → `tddy_session_agents::clone_readiness` (`205c0162`)

| Measure | Result |
|---|---|
| Plans | `07a-refuse-unready-clone-extract.jsonl` (`extract_method`, tail lines 148–169), `07b-refuse-unready-clone-to-module.jsonl` (`extract_module`, `to_file`, `reexport: glob`), `07c-clone-readiness-to-session-agents.jsonl` (`move_module_to_crate`, `reexport: glob`) |
| Engine did | the whole body (22 lines): the free function and its signature (`session_id`, `record`, and `clone: Option<AgentClone>`, the value the host read), the call left in the host method, the module file with its imports, the crate move, `pub mod clone_readiness;` in session-agents and the caller re-pointed |
| Hand-written | nothing: this method needed **no state struct, callback trait or trait impl**. Gray-zone substitutions: **0** |
| Engine refusals on the way | `extract_variable` over `self.session_agent_clones`, the first route tried: `rust-analyzer's answer was unusable: rust-analyzer did not produce a \`let var_name\` to name` ([**new todo**](../todo/2026-09-25-restructure-extract-variable-expects-a-var-name-placeholder.md)). Warm `check --deep` of `07c` **hung** with both processes at 0% CPU, and the same check cold answered `no findings` ([**new todo**](../todo/2026-09-25-restructure-warm-check-hangs-on-a-module-an-earlier-apply-created.md)) |
| Applies that left the tree broken | `07a`: 6 × `E0433`, because the method's function-local `use …::AgentCloneState;` stayed behind ([**new cause**](../todo/2026-09-25-restructure-extract-method-leaves-a-function-local-use-behind.md)). `07c`: 1 × `E0433`, the destination's own extern name ([existing](../todo/2026-09-25-restructure-move-to-crate-leaves-the-destinations-own-extern-name.md)) |
| Hand fixes (build corrections) | the `use` moved from the method body to the file header, then dropped as unused once `07b` carried it; `tddy_session_agents::session_agent_clone::AgentClone` → `crate::…`; `pub(crate) fn` → `pub fn` ([existing](../todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md), item 3); the engine's `pub use tddy_session_agents::*;` and dangling `pub(crate) use clone_readiness::*;` became `use tddy_session_agents::clone_readiness;` ([existing](../todo/2026-09-25-restructure-move-to-crate-leaves-a-nested-modules-parent-glob-dangling.md)) |
| New edges | none |
| Consumers edited | none. No public path existed for the new function. No test reads either file by path |
| Production lines | lifecycle 20,290 → 20,270; session-agents 3,592 → 3,625 |
| `verify --against HEAD` | 397,652 → 397,658. The 6 gained are the new signature and its call ([existing](../todo/2026-09-18-restructure-verify-cannot-exit-zero-for-an-extract-module.md)) |
| `cargo check --all-targets` | clean on lifecycle, session-agents, kernel, `tddy-daemon-rpc`, `tddy-daemon`, `tddy-telegram-control` |
| Clippy `-D warnings`, `cargo fmt` | clean on lifecycle and session-agents |
| Tests | session-agents 72, kernel 122; lifecycle **562 passed, 22 failed, 1 ignored**, identical to the baseline by name |

#### Step 2: `agent_clone_for` with the state struct: **refused by the engine, rolled back**

This was the first body that needs the port: after `self.session_dir_for(…)?` it reads two host
fields. Hand-written, as the decision allows:
- `tddy_session_agents::AgentRosterState<'a>`, with the ten fields of the Phase 2 row, each borrowed
  (`&'a Arc<…>` as the host holds it, so no `Arc` or `DaemonConfig` is cloned per call);
- lifecycle's builder `DaemonSessionHost::agent_roster_state(&self)`, in `handler_state.rs`;
- in the body, one inserted call to the builder (`let state = self.agent_roster_state();`) and
  **2 gray-zone substitutions**, `self.session_agent_rosters` → `state.…` and
  `self.session_agent_clones` → `state.…`, token for token.

`cargo check` was clean. `extract_method` over the tail was then refused twice:
- **Warm:** `state: _`, because the daemon had never loaded the new file (a second symptom, added to
  the hang todo).
- **Cold:** rust-analyzer typed the signature correctly
  (`state: tddy_session_agents::AgentRosterState<'_>`), and the engine refused it anyway, as
  `rust-analyzer's answer was unusable: … `_` is not legal there (E0121)`. Its placeholder check
  splits on non-identifier characters, so the elided lifetime `'_` reads as a bare `_`
  ([**new todo**](../todo/2026-09-25-restructure-extract-method-refuses-an-elided-lifetime-as-an-untyped-placeholder.md)).

The plan is not malformed, so the step was stopped with nothing applied, and its hand-written
preparation was rolled back rather than left as unused wiring. An owned state struct would dodge the
check, but only by cloning every `Arc` and the whole `DaemonConfig` on each call, so it was not taken.
No callback trait was written, because no body that reached the engine calls one.

#### Step 3: `authorize_exec_tool_caller` → `tddy_session_agents::exec_tool_caller`

It was already a free function of `config` and `user_resolver`, so it is a pure move with no
extraction.

| Measure | Result |
|---|---|
| Plans | `08a-exec-tool-caller-to-module.jsonl` (`extract_module`, `to_file`, `reexport: glob`), `08b-exec-tool-caller-to-session-agents.jsonl` (`move_module_to_crate`, `reexport: glob`). The daemon was restarted between them, to avoid the step 1 hang |
| Blind-spot read before apply | no body path reaches a module that stays behind. `local_instance_id_for_config` and `DaemonConfig` are facades over `tddy-daemon-livekit` and the kernel, both already session-agents'. No name collides. Its sibling `resolve_exec_tool_worktree` stays (it calls `workspace_session`) and calls it through the facade |
| Engine did | the function with its doc comment, its six imports, re-pointed to the crates that define them (`tddy_daemon_livekit::…`, `tddy_daemon_kernel::…`), the sibling's call qualified, `pub mod exec_tool_caller;`, the move. Both applies passed the compile gate |
| Hand fixes | one unused import left in `svc_resolve_os_user.rs` ([existing](../todo/2026-09-24-restructure-apply-leaves-the-lint-gate-red.md)); the engine's `pub use tddy_session_agents::*;` + `pub use exec_tool_caller::*;` became `use tddy_session_agents::exec_tool_caller;` + `pub use tddy_session_agents::exec_tool_caller::authorize_exec_tool_caller;` ([existing](../todo/2026-09-25-restructure-glob-facade-re-exports-a-name-the-origin-shadows.md)). Gray-zone substitutions: **0** |
| Facade | `connection_service::authorize_exec_tool_caller`, which `tddy-daemon-rpc/src/exec_tool/ports.rs` imports, resolves unchanged |
| New edges | none |
| Consumers edited | none |
| Production lines | lifecycle 20,270 → **20,226**; session-agents 3,625 → **3,680** |
| `verify --against HEAD` | 397,658 before and after; the one changed statement is the engine's qualified call |
| `cargo check --all-targets` | clean on lifecycle, session-agents, kernel, `tddy-daemon-rpc`, `tddy-daemon`, `tddy-telegram-control` |
| Clippy `-D warnings`, `cargo fmt` | clean on lifecycle and session-agents |
| Tests | session-agents 72, kernel 122; lifecycle **562 passed, 22 failed, 1 ignored**, identical to the baseline by name. The function has no tests of its own; `context_rpc_session_scope_acceptance` names it only in a comment |

#### The rest of T3's scope: not moved, and why

| Item | Verdict |
|---|---|
| `resolvable_agent_defs`, `agent_def_for_spawn` (the T1 ↔ T3 cut) | **stopped: unapproved edge.** Both read `tddy_model_registry` (`ModelRegistryStore`, `registry_agent_defs`, `registry_agent_def_with_credential`), so session-agents would gain `tddy-model-registry`. That is no cycle, but it is 32 packages session-agents does not carry today, among them `tddy-acp`, `tddy-bsp`, `tddy-connectrpc`, `tddy-session-catalog`, `tddy-terminal-rpc` and the six `tddy-build*` crates. The free `resolvable_agent_defs` also calls `DaemonSessionHost::report_shadowed_agent_def`, an associated function of the host whose body holds an early `return` |
| `session_dir_for`, `ensure_session_room` | **wrong premise**: #524 already put them in child modules of their own (`svc_resolve_listed_worktree/session_dir_lookup.rs`, `…/session_room_opening.rs`), so there is nothing to split |
| `split_forward_deadline` → kernel (the T3 → T4 cut) | **not movable by the engine.** It is at `svc_spawn_split_agent.rs:411`; `svc_provision_agent_clone.rs:99` is a call site. Its body is one expression that reads `self.config`, so there is no `self`-free range, and `extract_variable` (which would hoist `&self.config`) is refused. `PEER_FORWARD_TIMEOUT` is already the kernel's. A lifecycle test calls `service.split_forward_deadline()`, so the method stays as a delegation either way |
| `worktree_snapshot` (`agent_roster.rs:40`) | already a port: it is lifecycle's `impl RemoteSnapshotSource for DaemonSessionHost` (livekit's trait). Nothing moves |
| `AgentHostCallbacks { worktree_snapshot; run_exec_tool_locally; local_exec_tools }` | **incomplete (wrong premise).** T3's bodies also call seven host methods defined outside T3: `common_room_slot` (6 calls), `session_dir_for` (2), `eligible_instance_ids` (2), `mint_first_admission_token`, `split_forward_deadline`, `resolve_exec_tool_worktree` and `ensure_session_room`. The first and third delegate to `peer_routing`, which the state carries, but re-pointing them there adds a token (`state.peer_routing.common_room_slot(…)`), which is outside the token-for-token gray zone |

#### Does the approach scale?

| Measure | Value |
|---|---:|
| Code lines in T3's `impl DaemonSessionHost` methods (43 methods, 6 files) | ~950 |
| of which sit in a `self`-free tail with no `return`, the only shape the engine moves today | ~260 (upper bound: 63 of them are the `service = self.clone()` closures, whose tails still name `DaemonSessionHost`) |
| Methods with such a tail longer than 5 lines, so worth a three-plan chain | about a dozen of 43. The longest are `remote_roster_record_for` (43), `ensure_project_available_for_start` (29), `session_room_participant_identities` (16), `forward_cancel_agent_conversation` (15) and `resolve_specialized_agent_defs` (15) |

The engine-to-hand ratio **was** healthy where the recipe applies. Steps 1 and 3 moved 66 lines
with 0 gray-zone substitutions, and about 6 and 3 lines of build corrections. But that recipe reaches
about a quarter of T3's host code, and it leaves each method's head, which is logic and not
wiring, in lifecycle. The remaining three quarters need the state-struct route, which three engine
defects block today:
- `extract_variable` is unusable, so no field read can be hoisted;
- an elided lifetime in any extracted signature is refused, so no borrowed state struct can be
  passed;
- a `return` anywhere in the range is refused, even when the range is the method's own tail and
  the return would mean the same thing. The `ret` rows are `provision_agent_clone`,
  `delete_clone_on_peer`, `forward_open_agent_conversation`, `claim_agent_clone`,
  `seed_session_agent_roster` and `report_shadowed_agent_def`.

Once the first two are fixed, the gray zone costs one builder call plus one substitution per field
read. For T3 that is dozens of substitutions, and the seven missing callbacks would need
multi-token rewrites. **So the approach does not scale to T4 or T1 as the engine stands.** Those two call into each
other (the `SplitHost` cut) and into the roster, so their bodies need callbacks by construction. The
next step is the developer's call: fix those three engine defects first (the pilot's todos name each), or widen
the gray zone to cover `self.<delegation>(…)` → `state.<field>.<method>(…)` and approve the callback
list above.

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
