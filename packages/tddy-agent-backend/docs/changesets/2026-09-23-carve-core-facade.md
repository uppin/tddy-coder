# 2026-09-23 — The crate is created from `tddy-core`'s coding-agent backends

**Type:** Refactor · `#carve` 12/14, PR [#522](https://github.com/uppin/tddy-coder/pull/522)
Cross-package entry: [`docs/dev/changesets/2026-09-23-carve-core-facade.md`](../../../../docs/dev/changesets/2026-09-23-carve-core-facade.md)

Created from `tddy-core`'s `backend/`, `stream/`, `token_accounting`, `claude_argv`, `claude_hooks`, `cursor_hooks` and `spawn_env`, moved with `git mv`. The backend names `tddy_workflow::{GoalHints, GoalId, PermissionHint}` directly (Cut 1), so it does not depend on the workflow engine. `backend::write_codex_thread_id_file` widened `pub(crate)` → `pub`, because the engine calls it. Workspace dependencies: `tddy-workflow`, `tddy-session-store`, `tddy-toolcall`. `agent-client-protocol` and `tokio-util` live here now. Six test binaries and five `complexity-*` records (all unchanged) moved in. `docs/cursor-ask-question-schema.md` moved from `tddy-core`. New code-issue records: `oversized-file-{claude,codex-acp,cursor,backend-mod,stub}` (767, 562, 519, 877 and 578 production lines, none grown). `tddy-core` re-exports it whole. See [architecture.md](../architecture.md).
