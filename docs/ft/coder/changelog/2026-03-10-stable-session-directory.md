# 2026-03-10 — Stable Session Directory

- **Output location**: Planning output always goes to `$HOME/.tddy/sessions/{uuid}/`. Each session gets a unique UUID subdirectory.
- **Discovery**: Removed `plan_dir_suggestion` from schema; planning prompt uses `name` (human-readable changeset name) instead.
- **Packages**: tddy-core (create_session_dir_in, SESSIONS_SUBDIR, PlanTask session_base), tddy-coder (run.rs output_dir handling).
