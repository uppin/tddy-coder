# 2026-03-28 — Session id validation on connection RPCs

**Type:** Feature

**`ConnectSession`**, **`ResumeSession`**, and **`SignalSession`** call **`validate_session_id_segment`** before **`unified_session_dir_path`**; aligns with **`session_deletion`** for **`DeleteSession`**. [connection-service.md](../connection-service.md), [session-layout.md](../../../../docs/ft/coder/session-layout.md). (tddy-daemon)
