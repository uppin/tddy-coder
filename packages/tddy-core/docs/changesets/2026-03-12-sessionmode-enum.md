# 2026-03-12 — SessionMode enum

**Type:** Refactor

Replaced `session_id: Option<String>` + `is_resume: bool` on InvokeRequest with `session: Option<SessionMode>`. SessionMode::Fresh(id) maps to --session-id; SessionMode::Resume(id) maps to --resume. Single type expresses session identity and mode. (tddy-core)
