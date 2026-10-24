# 2026-09-06 — An unprivileged daemon cannot probe another OS user

**Category:** Host tooling probe
**Source:** `#hosts-screen` 4/8, PR #456, 2026-09-06

- `spawner`'s privilege drop calls `libc::setgid` / `initgroups` / `setuid` directly in `pre_exec`,
  which requires privilege. Under `./install --systemd` the daemon runs as an **unprivileged child**
  of `tddy-supervisor`, so for any `os_user` other than the daemon's own the `pre_exec` returns
  `EPERM` and `GetHostTooling` reports a probe failure — honestly, but **AC-7 of the host-tooling
  work ("the probes run as the host's OS user") is not satisfiable on that deployment shape.**
- **Pre-existing**, not introduced here: `run_capture_as_user` has the same constraint, and its one
  existing caller (`list_agent_models`) lives under it. The tooling probe is simply the first
  per-host, multi-user path to exercise it, so it is the first place the limit is visible to an
  operator.
- Needs a **deployment decision**, not a code fix in isolation: either the supervisor brokers the
  spawn, or the daemon keeps a capability that lets it change user, or the feature is documented as
  single-user on supervised installs. Not verifiable locally, and not one node's call.

## Update 2026-10-24 — #734 (`#start-progress` 1/8)

Still open. The privilege drop now lives in `tddy_daemon_kernel::pre_exec_stages` and makes no
`initgroups` call, and a non-root daemon no longer forks into an `EPERM`: `plan_privilege_change`
refuses **before forking** with `CannotBecomeUser`, naming both users and the supervisor. The probe
still fails on the supervised deployment shape; the deployment answer is `#start-progress` 3/8's
supervisor handoff.
