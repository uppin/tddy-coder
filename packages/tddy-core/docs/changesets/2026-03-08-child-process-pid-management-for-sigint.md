# 2026-03-08 — Child Process PID Management for SIGINT

**Type:** Feature

Added global AtomicU32 child PID tracking (set_child_pid, clear_child_pid, get_child_pid, kill_child_process). ClaudeCodeBackend and CursorBackend register/clear child PID around spawn/wait. kill_child_process sends SIGKILL on Unix; returns false on non-Unix. (tddy-core)
