# 2026-09-23 — Destructured in place: no file over 500 production lines, twelve duplicates merged

**Type:** Refactor

`#carve` 14/15 ([#524](https://github.com/uppin/tddy-coder/pull/524)). Cross-package entry, with the
baseline, every plan applied, the hand fixes by gap, the DRY rows and the LoC assessment:
[2026-09-23-carve-lifecycle-destructure.md](../../../../docs/dev/changesets/2026-09-23-carve-lifecycle-destructure.md).

No behaviour change and no public-surface change: the baseline (61 targets, 622 passed, 22 failed,
1 ignored, the same 22 by name) held after every milestone, and every public
`tddy_session_lifecycle::…` path resolves as before (`spawn_blocking_with_timeout`'s `pub use`,
which plan `01`'s assist narrowed, was restored).

**Layout** (see [module-layout.md](../module-layout.md)):

- `connection_service.rs` 1,601 → 434 production lines: ten topic files (`placement`,
  `worktree_source`, `split_start`, `attachment_progress`, `managed_launch`,
  `stack_seed_validation`, `stack_child_spawn`, `conversation_spawn`, `roster_replacement`,
  `claude_cli_spawn`).
- `cli_session_manager` became a directory module (1,371 → 173, nine children).
- `svc_start_session_core.rs` 911 → 455, with five step modules; `start_session_core` 857 → 358.
- `session_coordinate_handlers.rs` 818 → 414 (resume, signal/delete moved out); `split_session`
  674 → 409 (`agent_argv`, `agent_credentials`); `cursor_cli_spawn` 524 → 445 (`chat`, `resume`).
- The sandboxed Claude start's jail steps (`jail_launch_steps`, `jail_session_files`,
  `jail_worktree`) and the relaunch's (`relaunch_jail_dirs`, `relaunch_jail_steps`) in modules.
- The two ports files split into adapters and `PeerRouted*` wrappers (644 → 113, 503 → 200).
- The host constructor and builders moved to `svc_resolve_tddy_tools_path/svc_host_builders.rs`
  (498 → 51).
- Plan `11`'s nine misplaced clusters cut into modules of their own.
- 13 parameter structs replace over-arity signatures the engine wrote.

**Shared definitions (DRY #2–#13, −563 lines here):** `spawn_tddy_coder(ToolSpawnPlan)`;
`cli_start_prelude`, `managed_recipe_for`, `attached_initial_prompt`; the `service_util`
helpers `starting_session_metadata`, `find_registered_project`, `project_repo_root`,
`index_session_worktree`, `write_initial_changeset`, `create_session_worktree`;
`tddy_daemon_kernel::trim_to_option` at 10 sites; the routed session-agent methods forward `req`;
`MpscResultStream` from `tddy-worktree-service`; `activity_delta_frames` as a delegate to
`tddy-session-activity`; `tddy_pty::strip_resize`; `PERMISSION_PROMPT_TOOL` from
`tddy-sandbox-recipes`.

**Measured** (inline-test-block rule): files ≥ 500 production lines 11 → **0** (largest 493);
≥ 1,000: 2 → 0; functions over 150 lines 10 → 5. Production lines 20,899 → 22,387 (+1,488: the moves
added `use`, `mod`, blank lines, signatures and parameter structs; the DRY rows removed 563).

**Code issues** (`5d8bd0e9`): the five `oversized-file-*` records deleted clean, with final numbers
`connection_service.rs` **434**, `session_coordinate_handlers.rs` **414**, `split_session.rs`
**409**, `svc_spawn_split_agent.rs` **445**, `svc_start_session_core.rs` **455**. Moved and renamed:
`resume_session_at_session_coordinate` (242 → 137), `start_split_claude_cli_session` (146),
`delete_paired_codebase_session`. Narrowed and kept: `start_session_core` 857 → 358,
`start_sandboxed_claude_cli_session` 615 → 342, `spawn_cursor_cli_session_inner` 337 → 244,
`spawn_split_agent` 254 → 110, `ensure_project_available_for_start` 158 → 99. Touched and open:
the sandboxed Cursor start's CRAP record (465 → 414, still never executed). Unchanged:
`handle_rpc` (147), `resume_claude_cli_session` (119).

**Deferred**, each in `docs/dev/todo/` with its reason: the five functions still over 150 lines
(E4, T, U), DRY #1 (the shared jail launch, which needs coverage first), the hand re-parenting and
folding moves, `session_entry_from_listing`, and the 14 files between 400 and 493.
