# 2026-07-27 — Unified worktree base resolution (cursor-cli PR-stack fix)

- Chain base resolution is unified in **tddy-core::session_chain**. `resolve_chain_base_ref` and its helpers (`parent_is_pr_stack_orchestrator`, `pr_stack_node_for_spawn`) move out of the daemon into core, and a new `resolve_chain_base_for_session_spawn` encodes the spawn-time precedence: a runtime `stack_parent` wins over a persisted `worktree_integration_base_ref`, which wins over the default base.
- A **Cursor CLI** session spawned with a PR-stack orchestrator parent no longer falls back to `origin/master`: the sandboxed and non-sandboxed spawn paths thread `stack_parent` through, record `Changeset.orchestrator_session_id`, and base the child off its planned node's effective base via `Stack::base_ref_for_spawn`.
- **Claude CLI** and **Telegram** spawn paths route through the same resolver; Telegram now honors the persisted `worktree_integration_base_ref` the branch callback wrote (the prior `None` pass ignored it and always used the default base).
- See [git-integration-base-ref.md § Session chaining](../git-integration-base-ref.md#session-chaining-parent-session--originbranch).
