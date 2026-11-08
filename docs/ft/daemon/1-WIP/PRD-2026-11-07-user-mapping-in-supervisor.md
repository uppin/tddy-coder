# The GitHub → OS-user mapping lives in `users.yaml`, owned by whoever runs the deployment — PRD

**Date**: 2026-11-07
**PRD type**: Requirements change (existing feature)
**Stack**: `#agent-tools` 31/32 — branch `feature/agent-tools/user-mapping-in-supervisor`

## Affected Features

- [session-auth.md](../session-auth.md), [github-identity.md](../github-identity.md): who a GitHub login runs as
- [daemon-settings.md](../daemon-settings.md): `UpdateConfig`, and first-login enrolment on desktop
- [systemd-install.md](../systemd-install.md): `./install --systemd` and `--user`, and the one-time migration
- [../../supervisor/tddy-supervisor.md](../../supervisor/tddy-supervisor.md): spawn policy, the per-user sockets, and restarting on a mapping change
- [../../desktop/tddy-desktop-tauri.md](../../desktop/tddy-desktop-tauri.md): a desktop install's mapping file

## Summary

The mapping from GitHub login to OS user moves out of `daemon.yaml` `users:` into a file of its
own, **`users.yaml`**. That file is read by whichever process owns the deployment:

- **supervised** (`./install --systemd`): the root `tddy-supervisor` reads it. The supervisor:
  - derives **one** per-user daemon socket for each mapped OS user;
  - hands the daemon, at exec, the **path** of `users.yaml` and one named listener per user (the
    daemon reads each socket's path from its listener);
  - restarts the daemon when the file changes.
- **unsupervised** (desktop, `--user`, dev): the daemon reads it itself.

The same file replaces three hand-kept lists in `supervisor.yaml`:
`spawn_policy.allowed_session_users`, `services[].host_sockets` and
`services[].agent_tool_sockets`. It also replaces the install-time survey that keeps them in step.

## Background

Today each OS user is named in up to four places:

- `daemon.yaml` `users:`;
- `supervisor.yaml` `spawn_policy.allowed_session_users`;
- `host_sockets:`;
- `agent_tool_sockets:`.

`./install` keeps the last one in step by parsing `supervisor.yaml` with `awk`
(`install:716-900`). A user missing from one list fails a start at run time, naming the setting.

Each user also costs **two** handed-over descriptors: a host-session socket and an agent-tool
socket. With 16 slots and descriptor 3 taken by the daemon's own listener, a deployment holds at
most seven session users.

The user's rule for the supervisor (2026-11-07, verbatim): "supervisor shouldn't be aware of any env
or coding agent specifics. It's job is to manage tddy-daemon on the correct OS user, manage cgroups
etc. It shouldn't inherit config that normally should belong to tddy-daemon." The mapping is
identity and authorization, which decides what OS users the supervisor may run things as. It is
not specific to any agent, so it fits that rule.

## Proposed Changes

### What's Changing

- **`users.yaml`** holds the rows `daemon.yaml` `users:` has today
  (`- {github_user: <login>, os_user: <account>}`).
  - Supervised: beside `supervisor.yaml` (`/etc/tddy/users.yaml`), root-owned, and checked for
    trust the way `supervisor.yaml` is.
  - Desktop: `~/.tddy/users.yaml`.
  - Dev: a checkout file that `./web-dev` and `./desktop-dev` render.
- **One per-user daemon socket.** For each mapped OS user, the supervisor creates one socket with a
  generic name at a deterministic path, `<users.sockets_dir>/<os user>/daemon.sock` (default
  `sockets_dir: /run/tddy-users`), owned by that user (directory `0700`, socket `0600`), announced
  as `user.<os user>` in `LISTEN_FDNAMES`. An unsupervised daemon binds the same shape itself at
  `<data dir>/run/<os user>/daemon.sock`. The
  daemon serves both the host-session services and the agent-tool roster on it. That frees one
  descriptor per user, so the cap is **15 users** (16 slots less descriptor 3). A 16th mapped user
  fails loading, naming `users.yaml`.
- **Handover at exec.** The supervisor passes the daemon (the service that declares
  `serves_users: true`):
  - the path of `users.yaml` (`TDDY_USERS_FILE`); the daemon reads the rows itself, and refuses a
    file that is not root-owned or that anyone but root could write;
  - the listeners (`LISTEN_FDS`/`LISTEN_FDNAMES`, `user.<os user>`). Each socket's path is read from
    its listener, so no path is handed separately.

  A supervised daemon derives no socket path of its own.
- **Session users come from the mapping.** The supervisor runs sessions only as an OS user the
  mapping names. `root` is still refused.
- **Re-read on change.** The supervisor watches `users.yaml` (the `notify` crate). On a change it
  does two things:
  - it rebuilds the per-user listeners, keeping the descriptor of every user who did not change;
  - it restarts `tddy-daemon` with the new mapping.

  A file that fails validation is refused and logged, and the running daemon keeps the old mapping.
- **Who writes the mapping.**
  - On desktop, first-login enrolment writes `users.yaml`, never `daemon.yaml`. `UpdateConfig`
    stops re-serialising `users:` into `daemon.yaml`; it carries no mapping field, so it leaves
    `users.yaml` as it is.
  - A supervised daemon never writes the mapping; enrolment there is refused.
- **Migration, once.** `./install` writes `users.yaml` from an existing `daemon.yaml` `users:`, then
  drops these keys:
  - `daemon.yaml` `users:`;
  - `spawn_policy.allowed_session_users`;
  - `host_sockets:`;
  - `agent_tool_sockets:`.

  After that, any of those keys fails loading with an error that names `users.yaml`.
- **Removed:** the three `supervisor.yaml` lists above, and `./install`'s
  `survey_agent_tool_sockets` and `declare_agent_tool_sockets`.

### What's Staying the Same

- The lookup itself: `DaemonConfig::os_user_for_github` answers `None` for an unmapped login. It has
  no default arm and its ~60 call sites are unchanged.
- The rule that enrolment writes only the first row, on desktop.
- The rest of the supervisor's policy (`allowed_tool_paths`, `allowed_relay_tool_paths`,
  `allowed_mount_roots`, `allowed_env_keys`) and the daemon's own listener at descriptor 3.

## Impact Analysis

### Technical Impact

- `tddy-supervisor`:
  - loading `users.yaml` and checking it for trust;
  - deriving the socket set;
  - the handover;
  - spawn-policy users;
  - the file watch and restart.
- `tddy-daemon-kernel`:
  - `LiveUsers` loaded from `users.yaml`, or from the handover;
  - the per-user socket, which merges `agent_tool_sockets` with the host-session socket;
  - the old-key errors.
- `tddy-agent-launch`: host-session socket adoption, merged with the agent-tool adoption.
- `tddy-session-lifecycle`: socket adoption at build.
- `tddy-daemon`: `UpdateConfig`, and enrolment on desktop.
- The `./install` script, the YAML templates, `./web-dev` and `./desktop-dev`.
- `tddy-e2e`: the install tests.

### User Impact

- An operator adds a user in **one** file. Saving it restarts the daemon.
- **A restart drops every live relayed agent and the daemon's in-memory state** (open vaults,
  room connections, inference subscriptions). Sessions survive on disk and resume.
- An existing install is migrated once by `./install`. A hand-edited old key afterwards is a clear
  load error.

## Acceptance Criteria

- [ ] A supervised deployment with `users.yaml` mapping `alice` and `bob` serves one socket per user. Each socket carries both the host-session and the agent-tool services, and `supervisor.yaml` lists neither user.
- [ ] The daemon learns the `users.yaml` path and every socket from the supervisor (each socket's path from its listener) and derives none itself.
- [ ] A session can run as a mapped OS user. An unmapped one, or `root`, is refused.
- [ ] Fifteen mapped users fit; a sixteenth fails loading, naming `users.yaml` and the limit.
- [ ] Editing `users.yaml` to add `carol` gives her a socket and restarts the daemon. `alice`'s and `bob`'s descriptors are unchanged, and a client blocked in `connect` on their sockets during the restart is served afterwards.
- [ ] An invalid edit to `users.yaml` is refused and logged, and the daemon keeps running on the old mapping.
- [ ] A `users.yaml` that a non-root user could write is refused, by the supervisor and by a supervised daemon.
- [ ] `./install` on an existing install moves `daemon.yaml` `users:` into `users.yaml` and drops the old keys. A second run changes nothing.
- [ ] Any of `users:` (`daemon.yaml`), `allowed_session_users`, `host_sockets` or `agent_tool_sockets` (`supervisor.yaml`) fails loading with an error naming `users.yaml`.
- [ ] On desktop, first-login enrolment and `UpdateConfig` write `users.yaml`, and `daemon.yaml` gains no `users:` key.
- [ ] A supervised daemon refuses to enrol or to write the mapping.

## References

- Changeset: `docs/dev/1-WIP/2026-11-07-user-mapping-in-supervisor.md`
- Successor: `supervisor-agent-agnostic` (`#agent-tools` 32), being planned separately.
