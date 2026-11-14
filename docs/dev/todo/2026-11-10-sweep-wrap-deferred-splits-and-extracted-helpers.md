# 2026-11-10 — `#agent-tools` sweep (#876) wrap: three files the Tier C extracts pushed past 500, and the helpers they left over budget

**Category:** Known limitation (refactoring debt)

**Source:** #876 (`#agent-tools` 33/33 sweep), `/pr-wrap` step 3.5 file-length gate and the wrap's
structural re-measurement (brace matching over every non-test source file the PR changed, against the
merge-base `20f475576`). The Tier C `extract_method`s brought their target functions under budget, but
each extraction added a signature and sometimes a struct beside the function, so three files crossed the
500-production-line budget. Five of the extracted helpers are themselves over a function threshold.

Deferred by the user, 2026-11-10 (#876 wrap): left for a later refactoring node

Sibling of `2026-11-10-sweep-deferred-function-extractions.md`, which holds the six functions the engine
refused during `[G]`. This entry holds what the wrap measured afterwards.

## Files over the 500-production-line budget

| File | Production lines (merge-base → head) | Record |
|---|---|---|
| `packages/tddy-pty/src/runtime.rs` | 451 → 523 | `packages/tddy-pty/docs/code-issues/oversized-file-runtime.md` |
| `packages/tddy-session-lifecycle/src/connection_service/svc_host_builders.rs` | 493 → 531 | `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-svc-host-builders.md` |
| ~~`packages/tddy-session-split/src/svc_spawn_split_agent.rs`~~ | 497 → 575 | Closed 2026-11-12 by #878 (`#orchestrators` 1/11): 618 → 422, launch helpers moved to `svc_spawn_split_agent/launch.rs` (224); record deleted |

## Extracted helpers over a function threshold

| Function | Measurement (2026-11-10) | Record |
|---|---|---|
| `tddy-pty` `runtime.rs:304` `spawn_child_and_pump` | 67 lines · depth 3 | `packages/tddy-pty/docs/code-issues/complexity-runtime-spawn-child-and-pump.md` |
| `tddy-pty` `runtime.rs:449` `spawn_pty_writer` | 40 lines · depth 5 | `packages/tddy-pty/docs/code-issues/complexity-runtime-spawn-pty-writer.md` |
| `tddy-supervisor` `supervisor.rs:328` `build_service_slots` | 65 lines · depth 3 | `packages/tddy-supervisor/docs/code-issues/complexity-supervisor-build-service-slots.md` |
| `tddy-terminal-rpc` `local_pty_relay.rs:101` `spawn_stdin_pump` | 65 lines · depth 4 | `packages/tddy-terminal-rpc/docs/code-issues/complexity-local-pty-relay-spawn-stdin-pump.md` |
| `tddy-supervisor` `lib.rs:133` `start_reaper_and_users_watch` | 61 lines · depth 2 | `packages/tddy-supervisor/docs/code-issues/complexity-lib-start-reaper-and-users-watch.md` |

One more row was added to an existing record: `ensure_project_available_for_start`
(`packages/tddy-agent-launch/docs/code-issues/complexity-svc-ensure-project-available-for-start-ensure-project-available-for-start.md`)
grew by one comment line when the stale `allowed_env_keys` wording was reworded (B6). No code changed.

## What would close it

Each record's `## What would close it` names the seam. The three file splits are engine `move_item` /
`move_impl_members` plans into a submodule of the same file, which `check --deep` should accept on macOS
(none of the code is behind `cfg(target_os = "linux")`). Run them as one refactoring node with a green
baseline before and after, and delete each record with its final numbers in that node's change-history entry.
