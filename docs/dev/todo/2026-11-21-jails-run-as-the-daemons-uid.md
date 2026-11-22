# 2026-11-21 — Sandbox jails run as the daemon's uid, not the session's OS user

**Category:** Feature gap (split out of `#tab-hardening`, developer decision 2026-11-21)
**Source:** `#tab-hardening` 3/4 (PR #991) — the planned fourth node, `jail-uid`, was split out and recorded here

A session's main jail and its tab jails are spawned by the daemon, so the Linux cgroups backend runs them under
`geteuid()`: the daemon's uid. A session owned by another OS user still runs its jail as the daemon.

**What would close it:** run a jail as the session's OS user through the root supervisor, which already brokers
`setuid` for tool sessions.

- **The supervisor's `SpawnSandbox`** has userns, mounts, a netns and a uid map taken from the target, but no
  profile, no egress shim and no copies; the jail's stdio would need `SpawnRelayed`'s relay socket.
- **Files across two uids**: the IPC socket, ready marker, plan, egress directory and the persistent agent home are
  written by one uid and read by the other.
- **`SandboxSessionState`** holds a `JailProcess` (`Owned(Child)` today, `tddy-daemon-sandbox::jail_exit_watch`);
  a supervised jail is a second variant (`Supervised { socket, pid }`) that waits, polls and kills through the
  supervisor. The exit watch already goes through `JailProcess`.
- **No fallback**: under a supervisor a failed uid switch must refuse the start, not run as the daemon. Without a
  supervisor (user and desktop mode) the daemon's uid stays, and that is documented.
- **Linux only**: macOS Seatbelt has no supervisor path. Supervisor availability is
  `SpawnBackendChoice::from_environment` (`SUPERVISOR_SOCKET_ENV`).

**Why deferred:** testable only with a VM (the cgroups jail and a root supervisor), which waits for VM-backed testing
([`2026-11-02-ci-runs-no-linux-jail`](2026-11-02-ci-runs-no-linux-jail.md)).
