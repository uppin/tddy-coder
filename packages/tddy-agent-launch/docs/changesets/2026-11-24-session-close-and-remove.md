# 2026-11-24 — Close a session: terminate every tab, remove every worktree

**Type:** Feature

`DeleteSession` ends every tab and conversation (`SessionTabEnding`), tears down clones from the record, leaves a closed session's target alone and removes its target worktree before the record; `SessionClosesInFlight` refuses a resume or delete while a close runs; `delete_session_as` split into `end_everything_running`, `delete_what_peers_hold`, `remove_the_target_worktree`. Full record: [`docs/dev/changesets/2026-11-24-session-close-and-remove.md`](../../../../docs/dev/changesets/2026-11-24-session-close-and-remove.md) (#1032).
