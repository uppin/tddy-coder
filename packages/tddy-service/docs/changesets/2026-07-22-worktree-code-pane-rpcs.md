# 2026-07-22 — **Worktree Code-pane RPCs

**Type:** Feature

`ListWorktreeDirectory` / `ReadWorktreeFile` on `ConnectionService`** — `proto/connection.proto` adds two session-token-validated unary methods for browsing a session's worktree: `ListWorktreeDirectory` (`session_token`, `project_id`, `worktree_path`, `rel_path` → `repeated WorktreeDirEntry{name,is_dir}`, one level, dirs-first, `.git`/`.gitignore`-excluded) and `ReadWorktreeFile` (→ `content_utf8`, `truncated`, `byte_size`, size-capped). Powers the web [Code pane](../../../../docs/ft/web/session-code-pane.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-service)
