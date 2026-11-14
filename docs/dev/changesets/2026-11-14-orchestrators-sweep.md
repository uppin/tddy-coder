# 2026-11-14 — The `#orchestrators` sweep pays down the stack's code issues and TODOs

**Type:** Refactor

`#orchestrators` 12/12, PR [#901](https://github.com/uppin/tddy-coder/pull/901), the stack's sweep
node. Every other node (#878–#888) has merged. Feature:
[PR stacking § Opening a project stack by id](../../ft/coder/pr-stacking.md#opening-a-project-stack-by-id),
[PR-stack package § Behaviour](../../ft/web/prstack-package.md).

## What changed

Closed: B1 (`stack_git_tools.rs` split), B3 (`project::add_to_host`), B4
(`session_hook::transport`), B5 (`shell_tool` / `search_tools` / `lsp_tools`), B7
(`session_list_entries` / `callback_data`), B9 (the `session_id` refusal, `CreateProjectStack`,
and the web "New stack" action), B10–B12 (the three web hand splits), T1 (the spawner scope
check) and T2 (the rotation comments). The `muse-agent.sh` `health --watch` race fix is in this
node's first commit.

Accepted as standing debt by developer consent, 2026-11-14, reason "restructure engine refused":
B2 (`pr_stack/ports.rs`), the B6 remainder (`tddy-tools/src/server.rs`) and B8
(`start_session_on_request` / `start_here_body`). The engine gaps are recorded in
`docs/dev/todo/2026-11-14-restructure-engine-path-module-blocks-moves.md`,
`docs/dev/todo/2026-11-14-restructure-engine-no-extract-for-guarded-ranges.md` and
`docs/dev/todo/2026-11-14-restructure-engine-value-returning-arms.md`.

## Code issues — final measurements of the deleted records

| Record | Before → after (production lines) |
|---|---|
| `packages/tddy-daemon-rpc/docs/code-issues/oversized-file-stack-git-tools.md` | 999 → 432 |
| `packages/tddy-daemon-rpc/docs/code-issues/oversized-file-project-coordinate-handlers.md` | 566 → 444 |
| `packages/tddy-tools/docs/code-issues/oversized-file-session-hook.md` | 547 → 430 |
| `packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md` | 893 → 475 |
| `packages/tddy-telegram-control/docs/code-issues/oversized-file-telegram-session-control-callbacks.md` | 687 → 405 |
| `packages/tddy-session-web/docs/code-issues/oversized-file-sessions-drawer-screen.md` | 625 → 468 |
| `packages/tddy-prstack-web/docs/code-issues/oversized-file-pr-stack-screen.md` | 731 → 395 |
| `packages/tddy-session-create-web/docs/code-issues/oversized-file-create-session-pane.md` | 591 → 369 |

Kept open, narrowed or unchanged: `oversized-file-pr-stack-ports.md` (868 → 876),
`oversized-file-server.md` (2,289 → 2,024, partially fixed),
`complexity-svc-start-session-core-start-session-core.md` (83 / 273 lines, unchanged).

## Resolved backlog

- `docs/dev/todo/2026-11-13-spawner-scope-check-reads-changeset-with-ok.md` — deleted (T1).
- `docs/dev/todo/2026-11-14-conversation-rotation-residual-drift.md` — item 4 struck (T2);
  items 1, 2, 3 and 5 stay open for the TODO gate.
