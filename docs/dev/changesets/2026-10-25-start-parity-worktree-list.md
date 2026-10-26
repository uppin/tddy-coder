# 2026-10-25 — ListWorktrees and GetWorktree match the Worktrees screen

**Type:** Feature

`#start-parity` 3/9 ([#752](https://github.com/uppin/tddy-coder/pull/752)). Two project-home tools
read the snapshot the Worktrees screen's `StreamWorktreeStats` opens with, through one shared
function, from the daemon's one size calculator. Across:

- `tddy-tool-engine` — `project_home_worktree_tools.rs`: the defs, the screen's sort mirrored in Rust,
  cursor paging and `ProjectHomePort::list_worktrees`.
- `tddy-worktree-service` — `worktrees::project_worktree_snapshot`, `service::project_main_repo_on_host`,
  `DAEMON_SIZE_WALK_PERMITS`; `StreamWorktreeStats` builds its first frame from the shared snapshot.
- `tddy-session-lifecycle` / `tddy-daemon` — the host carries the worktree service's calculator.
- `tddy-daemon-rpc` — `DaemonProjectHome::list_worktrees`.
- `tddy-web` — `worktreeToolParity.cy.ts` pins the shared sort fixture to the screen's own sort.

Known limitation: the name tie-break reproduces `localeCompare` for ASCII names only. Code issues:
`tddy-worktree-service/src/service.rs` 772 → 808 and `worktrees.rs` 975 → 1003 production lines,
`tddy-tool-engine/src/lib.rs` 878 → 879 — splits deferred (stack-shared). Backlog:
`docs/dev/todo/2026-10-25-project-home-worktree-clear-delete-restore.md`.
