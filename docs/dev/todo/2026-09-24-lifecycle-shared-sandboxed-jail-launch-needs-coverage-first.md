# 2026-09-24 — the shared sandboxed jail launch (DRY #1) waits on tests that actually run the three callers

**Category:** Future enhancement
**Source:** `#carve` 14/15, [#524](https://github.com/uppin/tddy-coder/pull/524), change history
[`2026-09-23-carve-lifecycle-destructure`](../changesets/2026-09-23-carve-lifecycle-destructure.md)
(DRY row #1, "Consent list" item 4)

## What is left

The sandboxed Claude start, the sandboxed Cursor start and the sandboxed relaunch are three copies
of one launch-and-register sequence: jail dir, context dir, `canonicalize_exec`, semantic-index env,
warm-up, spawn/ready/bridge/state, metadata. Before the destructure, **76% of the Cursor function
(331 of its 435 lines) matched the Claude one line for line**, and the relaunch repeated about 80
lines of it. The plan was one `svc_sandboxed_jail_launch.rs`, with what differs (env builder,
mounts, `session_type`, `hook_token`) as explicit parameters and no `SandboxedAgentKind`
abstraction: Claude first, then Cursor, then relaunch.

The destructure **staged** it and stopped before the merge. The Claude and relaunch steps are each
their own functions, in modules of their own:

| Caller | Staged helpers |
|---|---|
| `start_sandboxed_claude_cli_session` (342 lines) | `svc_start_sandboxed_claude_cli_session/jail_launch_steps.rs`: `warm_up_jail_agents`, `managed_jail_env`, `jail_semantic_index_env`, `launch_jail` · `jail_session_files.rs`: `prepare_jail_dirs` (→ `JailDirs`), `prepare_jail_context_dir`, `write_jail_session_metadata` · `jail_worktree.rs`: `project_default_branch_ref`, `create_jail_project_worktree`, `link_jail_branch_to_stack_node`. Parameter structs `JailSession<'a>`, `JailDirs`, `JailLaunch`, `JailBranch<'a>`, `JailRunnerEnv<'a>`, and `type ManagedJailEnv` |
| `relaunch_sandboxed_runner` (149 lines) | `svc_relaunch_sandboxed_runner/relaunch_jail_dirs.rs`: `prepare_relaunch_dirs`, `refresh_relaunch_context_dir` · `relaunch_jail_steps.rs`: `relaunch_managed_workflow`, `resolve_relaunch_binaries`, `relaunch_jail_env`, `spawn_relaunched_runner`, `bridge_relaunched_jail`. Structs `RelaunchJailEnv`, `RelaunchedRunnerSpawn`, `RelaunchedJailBridge`, `type RelaunchManagedEnv` |
| `start_sandboxed_cursor_cli_session` (414 lines) | **none**. It shares only the `service_util` helpers of DRY #3/#5/#6/#7 (project lookup, starting metadata, `index_session_worktree`, `write_initial_changeset`, `create_session_worktree`) |
| all three | the env builders `specialized_subagent_env`, `jail_daemon_identity_env`, `lsp_tools_env` in `svc_turn_end_reporter/jail_env_builders.rs` |

Where the copies sit today, for example the context-dir step, already cut to the same shape on
two of the three callers (the argument order is all that differs):

```rust
// Claude: svc_start_sandboxed_claude_cli_session/jail_session_files.rs:49
pub(super) fn prepare_jail_context_dir(
    started_agents: &[tddy_core::SessionAgentRecord],
    worktree_path: &Path,
    context_dir: &Path,
) -> Result<(), Status> {
    let replacement_pairs = roster_replacement_pairs(started_agents);
    …

// Relaunch: svc_relaunch_sandboxed_runner/relaunch_jail_dirs.rs:38
pub(super) fn refresh_relaunch_context_dir(
    worktree_path: &Path,
    agents: &[tddy_core::SessionAgentRecord],
    context_dir: &PathBuf,
) -> Result<(), Status> {
    let replacement_pairs = roster_replacement_pairs(agents);
    …

// Cursor: still inline in start_sandboxed_cursor_cli_session, between its worktree cut and its argv
```

The merge would take each pair to one function, parameterised where the pair differs, and point the
Cursor start at it.

## Why deferred

**Missing coverage.** The three callers have no passing test on this host. Their suites are in the
destructure's known-red baseline, all with the same panic:

```text
sandbox RPC bridge not installed — runtime must call install_sandbox_rpc_bridge
    (packages/tddy-session-lifecycle/src/connection_service/svc_resolve_tddy_tools_path/svc_host_builders.rs:137)
```

| Suite | Red |
|---|---:|
| `sandboxed_claude_cli_acceptance` | 5 of 5 |
| `sandboxed_cursor_cli_acceptance` | 4 of 4 |
| `sandboxed_session_lifecycle_acceptance`: `delete_sandbox_session_stops_child_and_removes_directory`, `resume_sandbox_session_respawns_and_updates_pid` (the relaunch) | 2 |
| `sandbox_behavior_acceptance` | 5 of 5 |

The harness never installs the bridge, so every sandboxed-start path panics before it launches.
A hand merge of three launch paths would land with nothing exercising any of them.

[`crap-svc-start-sandboxed-cursor-cli-session`](../../../packages/tddy-session-lifecycle/docs/code-issues/crap-svc-start-sandboxed-cursor-cli-session.md)
says the same about the Cursor start alone: **"Restructure: no — tests first"** (CRAP 1,722, never
executed by any test).

**Characterisation tests were not written in #524, and that is why this is deferred.** The
developer's rule (2026-09-24) was characterisation tests "only if a seam needs one". No seam the
destructure cut needed one: every applied plan was a pure engine move, re-proven against the
baseline, and the DRY rows merged only copies proven identical. DRY #1 is the one change that
does need them, because it merges code paths that differ, so the tests belong to it.

## What would close it

1. **Tests first.** Either fix the harness so the sandboxed suites install the sandbox RPC bridge
   and go green on the host that runs them, or write characterisation tests for all three callers
   (the Cursor start's record requires them). Record the Cursor start's coverage in its CRAP record.
2. Merge Claude and relaunch onto one set of jail steps (they are already cut to matching shapes),
   the differences as parameters.
3. Point `start_sandboxed_cursor_cli_session` at the same steps, including the runner-argv builder
   ([U](./2026-09-24-lifecycle-functions-still-over-150-lines.md#u--the-runner-argv-ranges-stay-inline)).
4. Re-measure: both sandboxed starts under 150 lines, and the Cursor CRAP record re-derived and
   closed or narrowed.
