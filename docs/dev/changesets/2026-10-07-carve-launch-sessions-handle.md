# 2026-10-07 — The launch spawns and jail starts run over an owned `LaunchSessions` handle and a `LaunchHost` port

**Type:** Architecture

`#carve` 19/21 (#534, plan label 16d). The stack, child and conversation spawns (topic 9) and the jail and CLI-spawn
half of the agent launch (topic 1: the sandboxed Claude and Cursor starts, the relaunch and sandboxed resume, the jail
env builders, the Claude CLI start) are `impl LaunchSessions`, an owned handle over the host's launch fields, in place.
None of those modules names `DaemonSessionHost` or a wiring module. No behaviour change, no crate move, no consumer
edit (`tddy-daemon-rpc`, `tddy-daemon`, `tddy-telegram-control`, `tddy-desktop`), no new crate edge. Third conversion
after the agent topic and the split topic
([`2026-10-07-carve-split-sessions-handle`](../../../packages/tddy-session-lifecycle/docs/changesets/2026-10-07-carve-split-sessions-handle.md)).

Durable description: [`module-layout.md`](../../../packages/tddy-session-lifecycle/docs/module-layout.md#launch-sessions).

## State B

- `LaunchSessions` (`connection_service/launch_ports.rs`) holds nine host fields under the host's names, plus the
  agent topic's `AgentRoster` handle and `host: Arc<dyn LaunchHost>`. It is `Clone`, built per call
  (`launch_sessions()` in `handler_state.rs`). The plan's borrowed `LaunchState` and the owned handle are one struct,
  as `SplitSessions` is.
- `trait LaunchHost` = {`sandbox_rpc_handler`, `pr_stack`}, defined once, implemented once on the host in
  `svc_agent_host_ports.rs`. The seed-clone claimant is not a callback (it holds the roster handle).
- `impl StackParentHost` moved from the host onto the handle; `StackChildSpawnHandler` and
  `GrillMeConversationSpawnHandler` hold it. The three `Arc::new(self.clone())` hand-offs in
  `svc_start_claude_cli_session.rs` stay textually identical and clone the handle (Recipe B).
- `resume_sandboxed_claude_cli_session` moved by `move_item` into `svc_resume_sandboxed_claude_cli_session.rs`
  (an `extract_module` would have made it a child of the split module).
- `svc_launch_delegators.rs` holds the two `#[cfg(test)]` forwards a test still calls on the host. The start and resume
  half of the launch (`cli_branch_starts`, the Claude CLI resume) builds the handle from the host.
- The free CLI-spawn files name foundations by their defining crate (A4).

## Decisions

- **D1:** Recipe B (methods on a per-topic owned handle), as for the earlier topics. A5 amended: the grep matches
  exactly the three Recipe B lines, each cloning `LaunchSessions`.
- **D3:** `LaunchHost` stays at the plan's two methods; `StackParentHost` moves onto the handle.
- **D10 (a):** the unexecuted sandboxed paths are converted by the header-only edit, with Linux CI's sandboxed suites as
  the evidence. `TODO(crap-svc-start-sandboxed-cursor-cli-session)` keeps the deferred "tests first" rule.
- **A9 accepted outcome:** `start_sandboxed_cursor_cli_session` is **421 lines** (416 at the base): rustfmt wrapping from
  the longer re-pointed paths and the added `.agent_roster` hop. The written bound (<= 414) was below the base and
  unreachable by a token-only edit; the developer accepted 421.
- **`connection_service.rs` 503 -> 508:** three `mod` lines. Splitting it is deferred until the stack lands because
  #535 and #536 also edit it (developer consent, 2026-10-07).

## Before and after

| | Before (`7abe4a74`) | After |
|---|---|---|
| `tddy-session-lifecycle` suite (macOS, `--no-fail-fast`, `--test-threads=1`) | 575 passed, 22 failed, 1 ignored | same, the same 22 by name |
| `tddy-session-agents` | 75 passed | 75 passed |
| CI on `bd98e6ec` | | Rust 8662/8662, e2e 420/420, web 2803/2803; build, lint, arm64 and VM jobs pass |
| `start_sandboxed_claude_cli_session` | 343 lines | 343 |
| `start_sandboxed_cursor_cli_session` | 416 | 421 (accepted) |
| `relaunch_sandboxed_runner` | 149 | 150 |
| `svc_start_sandboxed_claude_cli_session.rs` | 496 production lines | 494 |
| `connection_service.rs` | 503 | 508 (deferred) |

Linux CI runs the sandboxed suites the 16 macOS-failing tests cover, so the start, relaunch and delete paths rest on
compiling, the edit rule and that CI run, which is green. `restructure verify --against 7abe4a74`: 27 statements lost,
each a hand re-point; it cannot exit zero for hand edits.

## Code issues

Rows re-measured and added, none deleted, none closed: `complexity-svc-start-sandboxed-claude-cli-session-…`,
`crap-svc-start-sandboxed-cursor-cli-session`, `complexity-cursor-cli-spawn-spawn-cursor-cli-session-inner` (not grown),
`oversized-file-connection-service` (deferred; backlog entry is the status section of
`docs/dev/todo/2026-09-24-lifecycle-files-over-the-400-line-target.md`).

## Open items for the next nodes

- 16e (#535) converts the start and resume half and the coordinate handlers and calls these functions through the handle.
- The move node (#536) moves the launch topic with the engine only; the free files call `hooks_and_urls` free functions
  whose file is 16e's (edges that leave with it).
- Re-measure `connection_service.rs` after the stack lands.
