# 2026-10-08 — about twenty items the `#carve` 21 moves widened to `pub` have no user outside their own crate

**Category:** Future enhancement: public-API hygiene after a move (INFO, found by validation)
**Source:** #carve 21/21 (PR #536), "Public-API widening" in `/validate-changes`. A counter over `git diff -M -U0` of the node found
**275** `pub(crate)`/private → `pub` widenings (166 in `tddy-agent-launch`, 49 `tddy-session-agents`, 37 `tddy-session-split`,
10 `tddy-demo-vm-service`, 7 `tddy-session-files`, 3 `tddy-session-activity`, 1 each elsewhere). The widenings were build corrections
(the engine moves an item into another crate and the origin still names it), which the node's rule allows: "`pub(crate)` → `pub` on items that now cross a crate".

## What was checked (grep over `packages/`, 2026-10-08)

For each item below, **every** occurrence (`grep -rnw --include='*.rs'`) is inside the crate that defines it. Nothing in lifecycle, in the
consumers or in any other receiver names it. That means `pub(crate)` or private would compile for callers; whether it does for
*signatures* (a `pub` item whose signature mentions the type, which then triggers `private_interfaces`) was **not** checked.

| Item | Defined at | Crate |
|---|---|---|
| `RelaunchJailEnv`, `RelaunchedRunnerSpawn`, `RelaunchedJailBridge` | `svc_relaunch_sandboxed_runner.rs:15,26,40` | `tddy-agent-launch` |
| `JailSession`, `JailBranch`, `JailDirs`, `JailLaunch` | `svc_start_sandboxed_claude_cli_session.rs:28,39,54,63` | `tddy-agent-launch` |
| `CliStart` | `svc_start_session_core.rs:42` | `tddy-agent-launch` |
| `ToolSpawnPurpose` | `tool_spawn_plan.rs:7` | `tddy-agent-launch` |
| `spawn_tddy_coder` | `tool_session_spawn.rs:154` | `tddy-agent-launch` |
| `bridge_conn_resume_response` | `session_coordinate_handlers.rs:34` | `tddy-agent-launch` |
| `SessionIdentityRefusal` | `session_acting_identity.rs:26` | `tddy-agent-launch` |
| `materialize_session_attachments` | `session_attachment_materialization.rs:38` | `tddy-session-files` |
| `NATIVE_FILESYSTEM_TOOLS` | `split_session.rs:32` | `tddy-session-split` |
| `from_forward_error` | `split_start.rs:36` | `tddy-session-split` |
| `get_demo_vm_status_at_coordinate` and the start/stop siblings | `demo_vm_coordinate_handlers.rs:196` | `tddy-demo-vm-service` (named only by `demo_vm_service.rs` in the same crate) |

The list is the validator's, re-verified by grep; it is **not** exhaustive of the 275. The other widenings were not checked for outside users.

## Why it is deferred

Narrowing is a visibility edit across files the move just produced. Doing it inside the move PR would mix a second kind of change into a diff
that is reviewed as "engine output plus build corrections". It also needs a compile per item to find the ones whose signatures forbid it.

## What would close it

Each item above is `pub(crate)` (or private) where the compiler accepts it, or is recorded here with the signature that forbids it. Then repeat the
grep for the other ~255 widenings. This is mechanical enough for `restructure`'s visibility intents if the engine grows one.
