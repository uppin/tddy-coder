# 2026-03-28 — DaemonService GetSession validation

**Type:** Feature

**`get_session`** rejects malformed **`session_id`** via **`validate_session_id_segment`** before joining **`sessions_base`** and **`SESSIONS_SUBDIR`**. Integration tests cover invalid ids and **`list_sessions`** visibility under the unified tree. [session-layout.md](../../../../docs/ft/coder/session-layout.md). (tddy-service)
