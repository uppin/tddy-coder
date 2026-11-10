# 2026-11-09 — The desktop's mapping is ~/.tddy/users.yaml

**Type:** Architecture

`#agent-tools` 31/32 ([#859](https://github.com/uppin/tddy-coder/pull/859)).

- `config_source.rs`: a dev load copies the `users.yaml` beside the config into the substituted temp copy (`CURRENT_USER` replaced); a missing one is left missing so the load names it, any other read error is the load's. `the_checkouts_desktop_dev_config_keeps_a_credential_vault` runs in a temp dir of its own (`tempfile` dev-dependency, already in the tree).
- `./install --desktop` renders an empty `~/.tddy/users.yaml` (`0644`); `./desktop-dev` renders the checkout-root one only for the checkout's own `dev.desktop.yaml`.
- Docs: [config-resolution-and-install.md](../config-resolution-and-install.md).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-09-user-mapping-in-users-yaml.md).
