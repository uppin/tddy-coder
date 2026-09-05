# 2026-07-04 — Daemon selector + LiveKit-only daemon-level RPC

- Every daemon-mode screen's header now shows a daemon selector, listing the daemons currently in the common LiveKit room; defaults to the daemon serving this web session.
- Switching daemons re-targets projects, worktrees, VMs, tasks, and session-list RPC at the selected daemon over LiveKit — no page reload, and no dependency on the target daemon's own HTTP origin.
- Per-session terminal and PR-Stack chat connections are unaffected — they keep talking to their own session's LiveKit room regardless of which daemon is selected.
- See [daemon-selector-livekit-rpc.md](../daemon-selector-livekit-rpc.md).
