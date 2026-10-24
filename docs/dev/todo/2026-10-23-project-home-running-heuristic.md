# 2026-10-23 — `EnsureProjectHomeSession` decides "running" by pid liveness or a served terminal

**Category:** Bug risk (low)
**Source:** `#project-home` 4/7 (`feature/project-home/home-session`, PR #699)

- `ensure_on_this_host` (`packages/tddy-agent-launch/src/svc_ensure_project_home.rs`) answers
  `running` — and returns the session unchanged — when the home session's recorded pid is alive
  (`session_reader::SessionEntry::is_active`) **or** `CliSessionManager::get(session_id)` still
  returns its main terminal. Otherwise it fast-forwards the home worktree and resumes.
- The served-terminal half is sound: a terminal's handle resolves only while its PTY task is in
  the registry, and `tddy-pty`'s runtime removes it when the child exits
  (`packages/tddy-pty/src/runtime.rs`), so an exited agent is not reported running.
- The pid half is the heuristic. A pid that is alive with **no** terminal served — the agent
  outlived a daemon restart, or the pid was reused by an unrelated process — reads as `running`:
  the pinned **main** entry selects a session whose terminal nobody serves, and the resume path
  is not taken until that pid dies. `ListSessions` has the same pid-reuse blind spot.
- **Why kept:** both cases need a daemon restart (or pid reuse) under a live home session; no
  spec pins either; and the fix is a liveness decision that belongs to the session listing, not
  to this node.
- **What would close it:** decide `running` from the served terminal alone on the daemon that
  owns the session (a pid with no served terminal treated as stopped, so ensure resumes it), with
  an acceptance case in `packages/tddy-session-lifecycle/tests/ensure_project_home_session_acceptance.rs`
  that records a live foreign pid in the home session's metadata and expects `resumed`.

**2026-10-24 (2026-10-24-project-home-from-new-session-form):** the same blind spot reaches
**Replace main**. Main is found by the project's recorded `home_session_id`, and a replace stops the
recorded main only when this daemon serves its terminal (`stop_previous_main`,
`packages/tddy-agent-launch/src/svc_ensure_project_home.rs`, which carries the code `TODO`). A main
whose process outlived a daemon restart is not served, so a replace leaves it running beside the new
main; it keeps its role, so it stays read-only. The fix above (liveness from the owning daemon) should
also let a replace stop that process, with a case in
`packages/tddy-session-lifecycle/tests/ensure_project_home_session_acceptance.rs` expecting the old
main stopped.
