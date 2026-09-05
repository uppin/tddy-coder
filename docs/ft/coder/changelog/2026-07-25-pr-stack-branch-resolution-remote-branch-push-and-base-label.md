# 2026-07-25 — PR-Stack: branch resolution, remote-branch push & base label

- A new **`QueryBranch`** RPC resolves the in-progress child session, on-disk worktree, and live GitHub PR status for one head branch; PR-Stack "Planned PRs" rows now render worktree + in-progress + PR from it (via `useQueryBranch`). Added additively — `GetPrStatus` / `usePrStatus` / `resolveNodeSession` are kept ([pr-stack-live-status.md](../pr-stack-live-status.md)).
- The Start-Session dialog gains a pre-checked **"Create Remote Branch"** toggle (claude-cli/cursor-cli, new-branch mode): the daemon `git push -u origin <branch>` after worktree creation and records `Changeset.remote_pushed = true`; a push failure fails session start ([session-drawer.md](../../web/session-drawer.md#create-session)).
- The dialog's new-branch option now names the concrete base — **"New branch from base: `<name>`"** — the predecessor stack branch for a stack node, the project default otherwise.
