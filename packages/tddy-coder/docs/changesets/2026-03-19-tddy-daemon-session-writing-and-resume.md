# 2026-03-19 — tddy-daemon Session Writing and Resume

**Type:** Feature

run_daemon writes .session.yaml on startup; updates session metadata. --resume-from and --session-id CLI args. When resume_from set, loads session from ~/.tddy/sessions/<id>/, sets plan_dir. When session_id set (daemon spawn), uses that for output_dir. Preserves daemon-provided session_id in run_main. ClientConfig gains daemon_mode for web app. (tddy-coder)
