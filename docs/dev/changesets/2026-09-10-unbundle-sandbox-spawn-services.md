# 2026-09-10 — The sandbox and spawn subsystems become their own crates

`#unbundle` node 3 of 8, PR [#472](https://github.com/uppin/tddy-coder/pull/472).

Nine modules and 5,600 lines leave `tddy-daemon` for two new crates, and one leaf RPC service goes
home to the crate that already owns its domain. No proto changed, no client migrated, and no
observable behaviour changed.

## What moved

| Destination | Modules |
|---|---|
| **`tddy-spawn`** *(new)* | `spawner.rs`, `spawn_worker.rs`, `supervisor_spawn.rs`, `supervisor_client.rs` |
| **`tddy-daemon-sandbox`** *(new)* | `sandbox_session.rs`, `workspace_tool_sandbox.rs`, `sandbox_action.rs`, `sandbox_plan_builder.rs`, `sandbox_runtime.rs` |
| **`tddy-bsp`** | `bsp_service.rs`, behind `build_bsp_service_entry(…)` |
| **`tddy-semantic-index`** | `semantic_index.rs` and its wiring suite |

Each cluster moved as **one unit**, because its modules reference each other. Git recorded the set at
96–100% rename similarity: five of the nine files have a literally empty diff, and the rest changed
only for import paths, log-target strings and intra-doc links. That similarity is the evidence the
code was relocated rather than rewritten, which is why nothing was split or reformatted on the way.

`tool_catalog_sync.rs` stops being a source file. Its entire body was one `#[cfg(test)]` module
declared in `lib.rs`, and it now lives in `tddy-daemon-sandbox/tests/`. It was relocated rather than
deleted: it guards the hand-copied exec-tool catalog that a later node collapses.

## `tddy-sandbox-app`'s dependency reverses

The clearest outcome. `tddy-sandbox-app` consumed `tddy_daemon::{sandbox_session, tool_engine}`;
it now depends on `tddy-daemon-sandbox` and `tddy-tool-engine`, and **`tddy-daemon` has left its
manifest entirely**. That included a log-filter *string literal* naming the old module path — a
change no compiler would have demanded and a silent loss of sandbox debug logging if missed.

`tddy-daemon` also drops `tddy-sandbox-qemu` and `tddy-sandbox-darwin` outright, and
`tddy-sandbox-cgroups` falls back to a dev-dependency.

## Two couplings broken without moving anyone else's code

- **`DaemonConfig` needed no decoupling at all.** It looks daemon-owned but is defined in
  `tddy-daemon-kernel` and only re-exported, as is `tddy_user_config`. The spawn subsystem reads the
  kernel directly, so **no signature changed** — which is what let `supervisor_routing.rs` move with
  its `spawn_backend_choice(&config)` calls intact.
- **`ManagedWorkflow` is type-erased, not moved.** The sandbox never called a method on it: the field
  is underscore-prefixed, assigned once, never read, and held only so a socket is cleaned up on drop.
  It is now `Option<Box<dyn SessionScopedResource>>`, with `tddy-daemon` stating a one-line opt-in
  impl. Dropping a `Box<dyn Trait>` runs the concrete destructor through the vtable, so teardown is
  unchanged and `session_toolcall.rs` stays where it is.

## The nextest exclusion audit

`[profile.ci]` excludes suites by `package(…) and binary(…)`, so a suite that changes package
silently stops being excluded and starts running. Exactly one of the five in-scope predicates needed
repointing — `sandbox_runner_stdio_acceptance` to `package(tddy-daemon-sandbox)` — and the excluded
set is otherwise identical in content. `docs/dev/guides/ci.md` was repointed to match.

## What this node deliberately did not do

Four planned moves were **declined on evidence**, each recorded in the changeset rather than forced:

- `task_service` → `tddy-task` and `action_service` → `tddy-actions` are impossible: both crates sit
  below `tddy-service` in the graph (`tddy-service → tddy-core → tddy-task`), so importing
  `tddy_service::proto::*` there is a dependency cycle. They stay in `tddy-daemon`. **Which node owns
  cutting that edge is unresolved** — this node's boundaries assign every cycle cut to node 1, while
  node 1 recorded four cross-crate cycles as belonging to "later nodes".
- A `semantic_index.SemanticIndexService` entry was planned for a service that does not exist: the
  module is three helpers, and there is no such service in any proto.
- Several sandbox suites and `supervisor_spawn_delegation.rs` stayed in `tddy-daemon`, because they
  mount `ConnectionServiceImpl` and moving them would make `tddy-daemon` a dev-dependency of the very
  crates being extracted. As a result `tddy-supervisor` survives in the daemon's dev-dependencies.

Deferred work is recorded as one file per item in [`docs/dev/todo/`](../todo/), including the
over-budget spawn modules, that test-file split, and two defects in `move_module_to_crate` that made
it unusable for these moves.
