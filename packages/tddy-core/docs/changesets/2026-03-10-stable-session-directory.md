# 2026-03-10 — Stable Session Directory

**Type:** Feature

When `--output-dir` omitted, session dir under `$HOME/.tddy/sessions/{uuid}/` instead of `output_dir/YYYY-MM-DD-slug/`. `create_session_dir_in(base)` in writer.rs; `SESSIONS_SUBDIR` constant. run.rs: `output_dir == "."` uses `$HOME/.tddy` + `create_session_dir_in`. workflow_runner: passes `session_base` when output_dir omitted. PlanTask: uses `session_base` from context. Removed `plan_dir_suggestion` from schema and DiscoveryData; planning prompt uses `name` instead. (tddy-core)
