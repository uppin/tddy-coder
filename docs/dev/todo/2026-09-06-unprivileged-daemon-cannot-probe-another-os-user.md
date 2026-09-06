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
