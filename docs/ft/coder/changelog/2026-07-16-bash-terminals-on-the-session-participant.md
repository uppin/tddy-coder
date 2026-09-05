# 2026-07-16 — Bash terminals on the session participant

- A tddy-coder session's LiveKit participant now serves the `terminal_id`-addressed terminal RPCs (start/stop/list/send/stream), spawning interactive bash shells in the session worktree via the new shared `tddy-pty` crate; the reserved `main` terminal remains the workflow VirtualTui and cannot be stopped via `StopTerminalSession`. See [session-participant-rpc.md](../session-participant-rpc.md).
- Bash terminals run the user's passwd login shell (not the process `$SHELL`, which under systemd/nix is not the user's interactive shell), so completion and rc setup match a normal login.
