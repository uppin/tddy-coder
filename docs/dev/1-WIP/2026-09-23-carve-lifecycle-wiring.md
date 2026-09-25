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
