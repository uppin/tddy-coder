# 2026-10-08 — a cross-crate move leaves `pub(crate)` items the origin still names (E0603 / E0624), widened by hand

**Category:** Manual fix after an engine move (build corrections, visibility only)
**Source:** #carve 21/21 (PR #536), R3: `svc_materialize_staged_attachment` → `tddy-session-files`. Engine cause: item 3 of
[2026-09-09-restructure-defects-from-the-first-cross-crate-move](2026-09-09-restructure-defects-from-the-first-cross-crate-move.md);
same shape as [2026-10-08-restructure-move-to-crate-leaves-a-pub-crate-fn-the-facade-caller-needs](2026-10-08-restructure-move-to-crate-leaves-a-pub-crate-fn-the-facade-caller-needs.md)

## What the engine did vs what was needed

The survey said "1 item(s) reached from outside: AttachmentState". The move left it, its four fields and the methods lifecycle calls
as `pub(crate)`, so `split_ports.rs`, `launch_ports.rs` and `attached_initial_prompt.rs` failed with `E0603`.

## The hand fixes (all `pub(crate)` → `pub`, in `packages/tddy-session-files/src/`)

- `svc_materialize_staged_attachment.rs:29-33`: `struct AttachmentState` and its fields `config`, `tddy_data_dir`, `staging_base_dir`, `peer_routing`.
- `session_attachment_materialization.rs:25` `prepare_session_attachments` and `:38` `materialize_session_attachments` (the two
  methods lifecycle calls; the four other methods stay `pub(crate)` because only these use them).

## What the engine should do

Widen every item the survey lists as reached from outside, and the fields and methods of a reached type that the origin's
remaining code names (the second half is also in [2026-10-04-restructure-move-item-does-not-widen-fields-or-impl-members](2026-10-04-restructure-move-item-does-not-widen-fields-or-impl-members.md)).

## Seen again in R4 (`tddy-session-activity`)

`presenter_observer_spawn.rs`: `PresenterObserverDeps` and its four fields (lines 6-10) and `maybe_spawn_presenter_observer` (line 23),
`pub(crate)` → `pub` (`E0603`, from `handler_state.rs` and `launch_ports.rs`, which stay in lifecycle).

## Seen again in R6 (`tddy-session-agents`, the 12-module T3 cluster)

The cluster move left every item lifecycle's remaining code names as `pub(crate)` or private, and the glob facade `pub use … ::*` re-exports only `pub`
items (so `started_roster_rev`, `SeededAgent` and others read as "not found in `connection_service`", `E0425`, besides `E0603`/`E0624`/`E0451`).
About 35 items were widened `pub(crate)` → `pub`, all in `packages/tddy-session-agents/src/`, found by a loop of `cargo check` and a name-based rewrite:
- `agent_host_callbacks.rs`: `AgentRoster` and its 12 fields, `AgentHostCallbacks`, `DaemonSeedCloneClaimant` and its `service` field.
- `agent_roster.rs`: `workspace_start_request`, `started_roster_rev`, `roster_agent_ids`, `refuse_unenforceable_withdrawal`, `agent_tool_reads_the_clone`.
- `peer_session_answer.rs`: `peer_has_no_such_session`, `resolve_worktree_root_in_session_dir`. `seeded_clone_guard.rs`: `SeededAgent` (+ fields).
  `seed_codebase.rs`: `ClaimedAgentClone`.
- Methods of `AgentRoster` the origin calls: `broadcast_roster`, `claim_agent_clone`, `claim_co_located_seed_clones`, `forward_cancel_agent_conversation`,
  `forward_open_agent_conversation`, `hosted_clone_for`, `local_agent_codebase_access`, `open_local_agent_session`, `open_owned_agent_session`, `read`,
  `refuse_departed_daemon`, `refuse_unready_clone`, `resolve_specialized_agent_defs`, `roster_record_for_agent_id`, `roster_session_dir`,
  `seed_session_agent_roster`, `seeded_roster_records`, `start_hosted_agent_clone`, `tear_down_agent_clone`, `tear_down_every_agent_clone`,
  `unwind_agent_clone_claim`, `unwind_seeded_roster`, and fields `codebase_session_id`, `commissioned` of the moved records.
The name-based rewrite also widened same-named `pub(crate)` fields/functions elsewhere in the receiver (`config`, `host`, `peer_routing` were already `pub`); check the diff for those.

## Seen again in R8 (`tddy-session-split`, 16 modules)

Widened by hand after the move, all `pub(crate)`/private → `pub` in `packages/tddy-session-split/src/` (plus `tddy-cli-sessions`), found by the same `cargo check` loop:
- Types and aliases: `SplitSessions` and its 11 fields (`split_ports.rs`), `SplitHost`, `SplitSessionFiles`, `SplitSessionAgents`, `SplitStartFailure` (+ `from_forward_error`),
  consts `NATIVE_FILESYSTEM_TOOLS` and `SPLIT_AGENT_TOKEN_TTL` (`split_session.rs`, plain private).
- Functions and methods lifecycle still calls: `create_session_worktree`, `find_registered_project`, `project_repo_root`, `starting_session_metadata`, `index_session_worktree`,
  `push_new_branch_to_origin_if_requested`, `resume_agent_and_recipe`, `workspace_sandbox_spec`, `write_claude_hooks_settings`, `write_initial_changeset`,
  `resolve_split_agent_placement`, `delete_paired_codebase_session`, `provision_workspace_tool_sandbox`, `resume_split_wiring`, `split_context_from_codebase_host`,
  `start_sandboxed_codebase_session`, `start_split_claude_cli_session`.
- `tddy-cli-sessions/src/cli_session_manager.rs`: `mod pty_handle;` → `pub mod pty_handle;` (split names `cli_session_manager::pty_handle::PtyHandle`).
