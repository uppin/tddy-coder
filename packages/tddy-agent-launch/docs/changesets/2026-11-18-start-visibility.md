# 2026-11-18 — A start on a target reports each call it ran and the worktree it cut

**Type:** Feature

- `cli_session_worktree::recorded_by(shell, progress)` wraps the start's target shell in a
  `RecordedShell` whose sink sends each call as the start's `remote_call` event; the cut reports
  `worktree_ready { branch, worktree }` once the checkout exists. `opencode_cli_spawn` wraps the
  target shell the same way for every later step (conversation, slot files, pointer, trust, launch).
- The sink holds the progress stream through a `WeakUnboundedSender`: a shell kept past the start
  neither holds the stream open nor reports after it (`recorded_start_shell_tests`).
- A remote clone error's text is scrubbed of credentials before it becomes the start's `Status`.
- Code issue `complexity-opencode-cli-spawn-start-cli-harness-session`: worse by 6 (391 → 397).

See [module-layout.md](../module-layout.md). Full record: [`docs/dev/changesets/2026-11-18-start-visibility.md`](../../../../docs/dev/changesets/2026-11-18-start-visibility.md).
