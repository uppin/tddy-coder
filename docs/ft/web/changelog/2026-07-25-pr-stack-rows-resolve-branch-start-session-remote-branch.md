# 2026-07-25 — PR-Stack rows resolve branch; Start-Session remote-branch push & base label

- Each PR-Stack "Planned PRs" row now shows its **worktree**, **in-progress session**, and **PR** link/state resolved by branch through the new `QueryBranch` RPC (`useQueryBranch`, per-branch polled) — additive alongside the existing `usePrStatus` / `resolveNodeSession` surfaces. See [session-drawer.md § PR-Stack Chat Screen](../session-drawer.md#pr-stack-chat-screen).
- The Start-Session dialog gains a pre-checked **"Create Remote Branch"** toggle (claude-cli/cursor-cli, new-branch mode) that pushes the new branch to `origin` at session start; unchecking skips the push. See [session-drawer.md § Create Session](../session-drawer.md#create-session).
- The new-branch option now reads **"New branch from base: `<name>`"** — the predecessor stack branch for a stack node (via `deriveStackBaseBranch`), the project default branch otherwise.
