# 2026-11-09 — The daemon reads its user mapping from users.yaml

`#agent-tools` 31/32 ([#859](https://github.com/uppin/tddy-coder/pull/859)).

- **`daemon.yaml` carries no `users:`.** The key fails loading, naming `users.yaml`; so does a missing
  `users.yaml` — never an empty mapping. Supervised, the daemon reads the file `tddy-supervisor`
  names in `TDDY_USERS_FILE` (root-owned, closed to group and others) and never writes it; otherwise
  it reads the `users.yaml` beside its config.
- **First-login enrolment writes `users.yaml`** on the desktop and leaves `daemon.yaml` unchanged;
  enrolment into a `users.yaml` that has gone is refused. A supervised daemon refuses to enrol.
- **`UpdateConfig` writes no mapping**: it never writes `users:` into `daemon.yaml`, and writes no
  `users.yaml`, since its message carries none.
- **`./install`** migrates an existing `users:` block into `users.yaml` once and renders an empty one
  when there is none; `./web-dev` and `./desktop-dev` render one for their runs.

See [session-auth.md § Who a login is](../session-auth.md#who-a-login-is-usersyaml),
[systemd-install.md](../systemd-install.md#behavior-notes) and
[daemon-settings.md](../daemon-settings.md).
