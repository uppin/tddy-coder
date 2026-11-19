# 2026-11-19 — `LocalShell`: this machine as a transport

**Type:** Feature

`LocalShell` runs `/bin/sh -c` here (tag `local`), held to the contract suite. `SshShell` and
`LocalShell` share `child.rs` for running and streaming a child. See
[architecture.md](../architecture.md) § The local transport. Full record: [`docs/dev/changesets/2026-11-19-session-driver-hooks.md`](../../../../docs/dev/changesets/2026-11-19-session-driver-hooks.md).
