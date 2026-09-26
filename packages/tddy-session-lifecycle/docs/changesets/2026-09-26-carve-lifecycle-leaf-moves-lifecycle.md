# 2026-09-26 — Host-free topics moved to their receivers, behind facades

**Type:** Refactor

`#carve` 15/21 ([#526](https://github.com/uppin/tddy-coder/pull/526)). Cross-package entry, with the
moves, the line counts, the baseline, the engine fixes and the rescope:
[2026-09-26-carve-lifecycle-leaf-moves.md](../../../../docs/dev/changesets/2026-09-26-carve-lifecycle-leaf-moves.md).

No behaviour change and no public-surface change: every public `tddy_session_lifecycle::…` path
resolves through a facade, and no consumer crate was edited.

- Moved out, with facades in `lib.rs`: `task_service`, `action_service` (`tddy-daemon-sandbox`);
  `relay_idle`, `local_token_tonic_adapter` (`tddy-daemon-kernel`); `pty_runtime`,
  `tddy_user_config` (`tddy-terminal-rpc`); `session_reader`, `user_sessions_path`,
  `session_deletion`, `session_list_enrichment` (`tddy-session-activity`); `peer_routing`,
  `session_admission_service` (`tddy-daemon-livekit`). `connection_service::attachment_progress`
  went to `tddy-session-files` behind a `pub(crate) use`.
- The agent roster's host-free code went to `tddy-session-agents`: `agent_records`,
  `authorize_exec_tool_caller` and the `self`-free tails of eleven host methods, which stay here as
  delegations. `DaemonSessionHost::agent_roster_state()` (`handler_state.rs`) lends those functions
  the `AgentRosterState<'a>` view.
- Three suites left with their code (`action_service_acceptance`, `action_sandbox_acceptance`,
  `worktree_removal_eligibility`): 59 → 56 suites. Baseline 622 → **562 passed, 22 failed, 1
  ignored**, the same 22 failures by name, the difference being the 60 tests that moved.
- Production lines ~22.8k → 20,041 (the move runs' counter); 22,393 → 19,618 and 122 → 109 non-test
  files by the inline-test-block rule of [module-layout.md](../module-layout.md).
- The host-bound rest of each topic is converted to per-topic ports and moved by
  [#531](https://github.com/uppin/tddy-coder/pull/531)–[#536](https://github.com/uppin/tddy-coder/pull/536).

Code issues: all eleven records re-measured, every one unchanged (numbers in the cross-package
entry); `complexity-svc-resolve-listed-worktree-ensure-project-available-for-start` gains an
"unchanged" row, since its file was touched.
