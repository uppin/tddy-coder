# 2026-08-15 — Rewriting the systemd units is a flag, not an env var

- **`./install --update-systemd-unit`** replaces the existing **`tddy-supervisor.service`** / **`.socket`** (or the **`--user`** **`tddy-daemon.service`**) with this script's templates. It replaces **`INSTALL_OVERWRITE_SYSTEMD_UNIT=1`**, which no longer does anything: what to install is an argument of the install, not part of its environment. Behavior is unchanged — without it an existing unit file is preserved. See [systemd-install.md](../systemd-install.md#flags).
