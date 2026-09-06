# 2026-09-06 — Loading a key on a supervised host needs a privileged path to the agent socket

**Category:** Host tooling probe
**Source:** `#hosts-screen` 5/8, PR #457, 2026-09-06

- `./install --systemd` runs the daemon as an unprivileged service account, and a user-session
  ssh-agent's socket is reachable only by its owner and by root — `/run/user/<uid>` is `0700`, and so
  is a shell-launched agent's `/tmp/ssh-XXXXXX`. So a supervised daemon can enumerate **its own
  account's** agent and no other host user's.
- Not the supervisor's env allowlist, which this node's planning documents blamed. `resolve_env`
  (`packages/tddy-supervisor/src/policy.rs`) gates the environment the daemon requests for a
  **session it asks the supervisor to spawn**; it says nothing about a socket the daemon opens in
  process, and a declared managed service is started with `EnvironmentBase::Inherited`.
- The **read** side degrades honestly: an unreadable candidate is still returned so `connect()`
  reports the permission error and the row reads "could not check" with it. Only `ENOENT` /
  `ECONNREFUSED` produce "no agent", and an empty key list is never fabricated.
- The **write** side (`#hosts-screen` 6/8, loading a key) has no such fallback — it needs a
  privileged path to the socket, e.g. the supervisor connecting after `setuid` to the target user and
  passing the connected fd back over the socket it already holds. That is a `tddy-supervisor` change
  and a **deployment decision**, not one the probe can take. It shares an account boundary with
  [an unprivileged daemon cannot probe another OS user](./2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md),
  by a different mechanism.
