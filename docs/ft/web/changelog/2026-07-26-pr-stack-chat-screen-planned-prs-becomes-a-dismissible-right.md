# 2026-07-26 — PR-Stack Chat Screen: Planned PRs becomes a dismissible right-side panel

- **The planned-PR list is no longer a fixed half-width pane.** It is now a **Planned PRs panel** — a docked 360px column to the right of the chat on desktop (open by default) and a full-screen overlay on mobile (closed by default), toggleable on both from the screen header and dismissible from its own close control. The chat gets the full width back. See [session-drawer.md § PR-Stack Chat Screen](../session-drawer.md#pr-stack-chat-screen).
- Each row now shows its **branch name**, or its planned branch name explicitly marked `planned:` — previously the branch was never rendered at all.
- A row whose base branch is missing from `origin` shows a blocked **"Missing branch: `<base>`"** indicator *instead of* the Start-session button, naming the branch it is waiting for; a row whose child session was deleted gets its **Start session** button back, pre-filled to **resume** the branch the node already owns.
- A PR lookup that could not be performed now reads **"PR status unavailable"** (reason on hover) rather than looking identical to "this branch has no PR".
- The screen makes **one** branch lookup per tick instead of two: `usePrStatus` is gone and `useQueryBranch` is the single source of live worktree / session / remote / PR state.
