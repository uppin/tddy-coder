# 2026-07-04 — Multi-daemon session host selection & auto-provision

- New sessions can pick which host/daemon runs them; the picker appears when more than one daemon is in the common room.
- "Add to host" now reliably reaches the host you chose, and lets you optionally set where the clone lands; each host shows its base clone location.
- Starting a PR-stack session opens the normal create-session form pre-filled from the planned PR, so you can review and adjust before it spawns.
- Starting a session on a host that doesn't have the project yet auto-clones it there (into the host's base location) instead of failing.
- See [projects-screen-multi-host.md](../projects-screen-multi-host.md), [daemon-selector-livekit-rpc.md](../daemon-selector-livekit-rpc.md), [session-drawer.md](../session-drawer.md).
