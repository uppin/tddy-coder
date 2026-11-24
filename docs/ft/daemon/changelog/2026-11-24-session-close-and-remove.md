# 2026-11-24 — Closing a session ends every tab and removes every worktree

- `CloseSession` terminates every agent tab of a session — main, tmux tabs here and on a target, jailed
  tabs, open subagent conversations — and removes every worktree it owns (its own, its conversations',
  a split session's codebase checkout, its agent clones), as one task; `PreviewCloseSession` says first
  what that would lose. The record and the branch are kept.
- Nothing unconfirmed is lost: a loss or an unreadable worktree asks first, a tab that survives its end
  stops every removal, and an agent that writes as it ends makes the close ask again.
- While a close runs, the session cannot be resumed, given a new tab or deleted. `DeleteSession` ends
  every tab and conversation too, tears down clones the record names, and deletes a closed target
  session without reaching its target.

Feature: [session-liveness.md § Closing a session](../session-liveness.md#closing-a-session)
