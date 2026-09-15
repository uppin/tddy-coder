# 2026-09-14 — LocalShell and RemoteShell exec backends

**Type:** Feature

Added `Shell` trait, `LocalShell`, `RemoteShell`, `session_shell`, and `execute_tool_on_shell` so
the exec catalog runs on the session host filesystem or over OpenSSH with `BatchMode=yes`.
