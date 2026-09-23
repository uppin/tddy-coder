# Initial discovery: carve-lifecycle-wiring

**Date**: 2026-09-23
**Changeset**: [2026-09-23-carve-lifecycle-wiring.md](./2026-09-23-carve-lifecycle-wiring.md)

This is a read-only survey of `packages/tddy-session-lifecycle` on top of #522 (`feature/carve/core-split`,
`0fb4fb85`). Nothing was built or edited to produce it.

## How lines are counted

A **production line** is any line of a `src/` file outside an inline `#[cfg(test)] mod x { … }` block.
A file whose module is declared behind `#[cfg(test)]` (the `*_tests.rs` files) counts as test code in full.

The first-`#[cfg(test)]` heuristic used by `/pr-wrap` step 3.5 gets this crate wrong. It stops
`connection_service.rs` at line 37, because that file's `#[cfg(test)] use` declarations come first,
so it counts 37 lines for a file with 1,647 production lines. It also counts the 19 test-only files
(4,185 lines) as production. See
[the file-length-gate entry](../todo/2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md).

| Measure | Lines |
|---|---:|
| Production | **21,264** (of which `test_util.rs`, a non-gated `pub mod`, is 366) |
| Tests inside `src/` | 7,570 |
| Integration tests (`tests/`, 59 suites) | 24,971 |

## Functionality by topic

This grouping comes from module docs and function names; the crate itself declares no topics.

| # | Topic | Production | Files |
|---:|---|---:|---:|
| 1 | Agent CLI start and resume: Claude and Cursor in PTYs, the sandboxed variants, `tddy-tools` path, hooks, local exec tools, agent-def resolution | 6,398 | 14 |
| 2 | RPC host core: the `DaemonSessionHost` struct, the `session.SessionService` handlers and adapter, the RPC-families port, shared helpers | 3,472 | 14 |
| 3 | Agent clones, roster and multi-agent rooms | 2,746 | 10 |
| 4 | Split and sandboxed-codebase sessions (agent here, worktree elsewhere) | 1,981 | 5 |
| 5 | Session catalog: list with enrichment, read, delete, notifications, workspace sessions | 1,397 | 6 |
| 6 | Terminals, PTY runtime, and the tasks and actions RPCs | 1,314 | 6 |
| 7 | Routing, peers, OS user, room admission, local token, relay idle | 930 | 5 |
| 8 | Attachments and session files | 919 | 2 |
| 9 | Stacked, child and conversation spawns, and PR-stack links | 814 | 5 |
| 10 | Activity ports and presenter observation (child `tddy-coder`) | 625 | 4 |
| 11 | Demo VM | 302 | 2 |

## Files at or over the 500-line budget

| File | Production | Needs extract-method? |
|---|---:|---|
| `src/connection_service.rs` | 1,647 | yes: `spawn_claude_cli_session_inner` :263-670 is **408** lines and takes 25 parameters |
| `src/cli_session_manager.rs` | 1,371 | no, a move is enough (the largest fn is 188) |
| `src/connection_service/svc_start_session_core.rs` | 911 | yes: `start_session_core` :54-910 is **857** lines |
| `src/connection_service/session_coordinate_handlers.rs` | 818 | yes: `resume_session_at_session_coordinate` :271-513 is **243** lines |
| `src/connection_service/svc_start_sandboxed_claude_cli_session.rs` | 661 | yes: one function, :46-660, **615** lines |
| `src/split_session.rs` | 651 | no |
| `src/connection_service/svc_session_agent_ports.rs` | 644 | no |
| `src/cursor_cli_spawn.rs` | 524 | yes: `spawn_cursor_cli_session_inner` :113-450 is **338** lines and takes about 27 parameters |
| `src/connection_service/svc_spawn_split_agent.rs` | 520 | yes: `spawn_split_agent` :57-327 is **271** lines |
| `src/connection_service/svc_start_sandboxed_cursor_cli_session.rs` | 505 | yes: one function, :40-504, **465** lines |
| `src/connection_service/svc_session_files_ports.rs` | 503 | no |

Close to the budget:

- `svc_resolve_tddy_tools_path.rs` is at 475. Most of it is the host constructor `new` (:65-159) plus
  the `with_*`/`set_*` builders, so the file name is wrong.
- `svc_split_context_from_codebase_host.rs` is at 457 production lines, 803 in total.

**Seven functions are too long to fix by moving them.** A move leaves a 600-line function at 600 lines,
so these need extract-method first.

## Per-file seams

### `connection_service.rs` (1,647 → ~390)

| Cluster | Lines | Size | Destination |
|---|---|---:|---|
| `spawn_claude_cli_session_inner` | 263-670 | 408 | `src/cli_spawn/claude.rs`. It is a free function that touches no host fields. Its 3 callers use `super::…`, so keep a `pub(crate) use` or repoint them |
| `ManagedLaunch`, `prepare_managed_workflow_inner` | 698-785 | 88 | `connection_service/managed_launch.rs` |
| `StackChildSpawnHandler` | 786-807 | 22 | `child_spawn_handler.rs`, which already holds its impl |
| `recipe_enables_conversation_spawn`, `conversation_branch_slug`, `GrillMeConversationSpawnHandler` | 823-876 | 54 | `conversation_spawn_handler.rs` |
| `roster_replacement_pairs` | 890-903 | 14 | `agent_roster.rs` |
| Attachment progress sink, reporter, materialization, cleanup | 904-1010 | 107 | `connection_service/attachment_progress.rs` |
| `CodebasePlacement` / `classify_*`, `WorktreeSource` | 1011-1173, 1582-1607 | ~190 | `connection_service/placement.rs`. The code is pure; keep `pub use` for `tddy-daemon-rpc/tests/*` |
| `SplitStartFailure` and split placement resolution | 1174-1284 | 111 | `connection_service/split_start.rs` |
| `activity_delta_frames` | 1382-1457 | 76 | **Stale duplicate** of `tddy-session-activity/src/service.rs:282`. Its only user is `tests/stream_agent_activity_delta_rpc_acceptance.rs:20`. Keep the headroom `const _: assert!` (1397) |
| `validate_stack_seed_base_session`, `session_repo_is_in_project` | 1458-1536 | 79 | `stack_parent.rs` |
| `MpscResultStream` | 64-100 | 37 | **Duplicate** of `tddy-worktree-service/src/stream.rs`. That copy first needs `into_receiver`, which `svc_session_lifecycle_ports.rs:41` calls |

What stays: the struct (127-236), `DaemonRpcHandler`, and about 100 lines of `mod` / `pub use`
declarations.

### `cli_session_manager.rs` (1,371 → ~170)

This becomes a directory module. Every cluster is an `impl CliSessionManager` block, so the fields stay as
they are.

| New file | Size |
|---|---:|
| `pty_handle.rs` | ~107 |
| `control_lease.rs` | ~100 |
| `argv.rs` | 67 |
| `launch.rs` | ~225 |
| `pty_spawn.rs` | 188 |
| `terminals.rs` | ~200 |
| `livekit_bridge.rs` | ~300 |

`strip_resize` (1126-1167) is byte-identical to `tddy-coder/src/session_participant/terminal_manager.rs:355`,
and a third variant sits in `tddy-sandbox-runner/src/runner.rs:1009`. It belongs in
`tddy_terminal_rpc::pty_relay`.

### `cursor_cli_spawn.rs` (524 → ~200)

- `chat.rs` gets hooks, `parse_created_chat_id` and `mint_cursor_chat_id` (30-112).
- `resume.rs` gets `resume_cursor_cli_session` (451-524).
- The spawn function (113-450) waits for the shared-helper extraction.

### `split_session.rs` (651 → ~350)

- `agent_argv.rs` (~190) gets the native-tool and permission-prompt constants, the roster withdrawals,
  and `split_claude_extra_args`. `PERMISSION_PROMPT_TOOL` duplicates
  `tddy-sandbox-recipes/src/claude_cli.rs:162`.
- `agent_credentials.rs` (~105) gets the token TTL, `mint_agent_session_token`, `verified_caller` and
  `RoomPollTokenMinter`.

### `svc_start_session_core.rs` (911): `start_session_core` alone is 857 lines

| Seam inside it | Lines | Size |
|---|---|---:|
| Checks before dispatch | 59-275 | ~220 (the extractable parts are `forward_start_session_if_remote` 85-136 and `refuse_unseedable_stack_base` 227-255) |
| Workspace branch | 277-427 | 151 (the no-clone path 302-405 becomes `start_seeded_workspace`) |
| claude-cli branch | 430-531 | 102 |
| cursor-cli branch | 534-633 | 100 |
| Tool (`tddy-coder`) branch | 635-910 | 276 |

- The tool branch's spawn-backend match (776-893, 118 lines) repeats
  `session_coordinate_handlers.rs:383-500` almost line for line. Only the `SpawnOptions` fields and
  the log labels differ. One `spawn_tddy_coder(ToolSpawnPlan)` replaces both.
- The claude and cursor branches share about 25 lines of prelude with each other and with
  `svc_spawn_split_agent.rs:68-88`. Those lines become `cli_start_prelude`, and the duplicated
  `managed_recipe` block becomes `managed_recipe_for`.

### `session_coordinate_handlers.rs` (818 → ~320)

- The `list` entry mapping (74-164) uses no `self`, so it becomes the free function
  `session_entry_from_listing`.
- Resume goes to its own file, where `spawn_tddy_coder` takes it to about 110 lines.
- Signal and delete (161 lines together) go to `svc_signal_delete_session.rs`.

### The sandboxed Claude, Cursor and relaunch paths are three copies

**76% of the Cursor start function matches the Claude one exactly** (331 of its 435 lines).

- The launch-and-register tail (about 160 lines in each) differs in only four places: the env builder
  name, `session_type`, `hook_token` and a comment.
- They really differ in worktree-source resolution (Claude 125 lines, Cursor about 50), in the argv,
  and in Claude's `append_system_prompt_file`.
- `svc_relaunch_sandboxed_runner.rs` is a third copy of the warm-up, context-dir, `canonicalize_exec`
  and spawn/ready/bridge/state steps (~80 lines).
- `svc_start_sandboxed_codebase_session.rs` (267) is **not** a copy. It combines a workspace start with
  `spawn_split_agent`.

The fix is `svc_sandboxed_jail_launch.rs` (~330), extracted method by method in this order:

1. (d) jail dir
2. (g) `canonicalize_exec`
3. (i) semantic-index env
4. (e) context dir
5. (a) warm-up
6. (j) `launch_jail`
7. (k) metadata

Pass the parts that differ as explicit parameters; do not add a `SandboxedAgentKind` abstraction.
Point Claude at the new file first, then Cursor, and relaunch only once it has test coverage for all
three callers.

**Behaviour risk:**
- (d, e, g, i, j): low.
- (f) managed env: medium, because of the prompt file.
- (k): medium, because of the two fields that differ.

### Claude and Cursor non-sandboxed spawns

About 155 lines are shared on each side (38% of the Claude function, 46% of the Cursor one):

- project lookup;
- initial changeset;
- local worktree with chain base;
- semantic index;
- trimmed prompt;
- `SessionMetadata`;
- push and response.

Across the crate, the semantic-index block appears in 5 files, `resolve_branch_workflow` in 5, and
`setup_worktree_for_session_with_optional_chain_base` in 6.

The shared helpers go in `src/cli_spawn/common.rs`, and one `CliSpawnRequest` replaces the parameter lists
of 25 or more arguments. After that the Claude function is about 180 lines and the Cursor one about 170.

### Ports files

- **`svc_session_agent_ports.rs` (644).**
  - Host impl 52-116, five port adapters 117-356, `PeerRoutedSessionAgents` 357-644.
  - The adapters call host methods, so they stay in this crate.
  - Each routed method copies its request field by field into an identical literal. Passing `req`
    saves about 6 lines in each of the 7 methods.
- **`svc_session_files_ports.rs` (503).** `PeerRoutedSessionFiles` (209-503) is a file of its own.
- **Both `PeerRouted*` wrappers could move down** into `tddy-session-agents` and `tddy-session-files`
  behind a forwarding port without creating a cycle. However, the module doc (:10-15) deliberately
  keeps routing here "rather than the crate growing a transport". That is a design decision for the
  developer.

### `svc_spawn_split_agent.rs` (520 → ~375)

- Teardown and delete of the paired codebase session (374-519) go to their own file.
- `spawn_split_agent` needs extract-method: the LiveKit room (126-173), the withdrawals and extra
  args (185-221), and the metadata (256-288).

## Coupling

- **Inside the crate.** Every `svc_*` file is a child of `connection_service`, so it reads
  `DaemonSessionHost`'s private fields directly (`connection_service.rs:127`). Moving code between
  siblings needs `pub(super)` at most.
- **Out of the crate.** Here the private fields are the obstacle. Each topic's impl block reaches into
  one struct, so moving it into another crate first needs a per-topic state or port struct. #520 set
  the pattern with `connection_service/handler_state.rs`.
- **Reverse dependencies.** `tddy-daemon-rpc`, `tddy-daemon` and `tddy-telegram-control` depend on this
  crate. `tddy-model-registry`, `tddy-tool-engine` and `tddy-worktree-service` use it only as a
  dev-dependency.
- **Existing crates below this one** that can take code without a cycle: `tddy-session-agents`,
  `tddy-session-files`, `tddy-session-activity`, `tddy-session-catalog`, `tddy-terminal-rpc`,
  `tddy-worktree-service`, `tddy-daemon-kernel` and `tddy-daemon-livekit`. None of them depends on
  lifecycle.
- **Cycle risk.** Anything that needs `DaemonSessionHost` itself cannot go below this crate without a
  port.
