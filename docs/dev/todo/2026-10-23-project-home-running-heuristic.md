# 2026-10-23 — A main that outlived a daemon restart is held by nothing, so a replace leaves it running

**Category:** Bug risk (low)
**Source:** `#project-home` 4/7 (`feature/project-home/home-session`, PR #699); narrowed by `#session-liveness` 1/4 (PR #958)

- **What remains:** *Replace main* (`stop_previous_main`,
  `packages/tddy-agent-launch/src/svc_ensure_project_home.rs`, which carries the code `TODO`) stops the
  recorded main only when the daemon holds it (`previous.is_active`, from the live set). A main whose
  process outlived a daemon restart is held by nothing until it is re-attached, so a replace leaves its
  process running beside the new main; it keeps its role, so it stays read-only. Stopping it would
  mean signalling the pid `.session.yaml` records, which the daemon cannot prove still names that
  agent.
- **What closed:** `running` is decided from the live set alone (`session_liveness`), so a main whose
  recorded pid is alive but whose terminal nobody serves is resumed rather than reported running
  (`ensuring_a_main_whose_recorded_pid_names_an_unrelated_process_resumes_it`), and `ListSessions`
  no longer reads a recycled pid as a live session.
- **What would close it:** make such a main held again — the eager re-attach of every live tmux-wrapped
  agent at daemon startup (`#session-liveness` 2/4) — with a case in
  `packages/tddy-session-lifecycle/tests/ensure_project_home_session_acceptance.rs` expecting a replace
  to stop an old main that outlived a restart.
