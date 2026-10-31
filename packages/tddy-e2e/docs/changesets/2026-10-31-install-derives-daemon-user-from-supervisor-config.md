# 2026-10-31 — Install derives the daemon user from supervisor.yaml

**Type:** Bug Fix

Seven functional e2e tests in `tests/install_supervisor.rs` cover the install decision paths: an upgrade derives the daemon identity from the existing `supervisor.yaml` (asserted through narration), `INSTALL_DAEMON_USER`/`INSTALL_DAEMON_GROUP` disagreeing with the preserved config are refused naming both values, a config whose `tddy-daemon` service declares no `user:` or `group:` is refused without a fallback, a config declaring `root` is refused naming the config, and fresh-install seeding is pinned. The fixture builder `a_supervisor_config_declaring_the_daemon_as(user, group)` plants operator configs. See [docs/ft/daemon/systemd-install.md](../../../../docs/ft/daemon/systemd-install.md). (tddy-e2e)
