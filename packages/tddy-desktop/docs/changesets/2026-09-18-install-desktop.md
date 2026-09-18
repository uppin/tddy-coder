# 2026-09-18 — A release build reads one file, and `./install --desktop` writes it

**Type:** Feature

`config_source::resolve` splits on the build profile. A debug build keeps every development rule
unchanged — `TDDY_DAEMON_CONFIG`, else `dev.desktop.yaml` found by walking up from the working
directory. A release build reads `$HOME/.tddy/desktop.yaml` and nothing else, and when that file is
absent it refuses to start and names `./install --desktop`. There is no fallback in either
direction: an installed `.app` launched from the Dock has `/` for a working directory, so anything an
upward walk found there it would have found by accident — most likely a developer's checkout.

`./install --desktop` writes that file, from the new repo-root `desktop.yaml.production` template,
and installs the application plus the CLI binaries the embedded daemon spawns. On macOS those
binaries land twice — inside `Contents/MacOS` for the sibling-of-`current_exe()` lookup, and in
`$BIN_DIR` for `allowed_tools` — and `$BIN_DIR/tddy-desktop` is a launcher script rather than a
symlink, because a symlink would make the process's executable path `$BIN_DIR/tddy-desktop` and break
both bundle identity and that lookup.

The template declares `listen.web_port` and no `web_bundle_path`. The port is not a served listener:
`runtime::build` requires it, and in this deployment it names the loopback port a sign-in comes back
on, which `oauth_callback.rs` opens for the duration of a sign-in and then closes.

The template leaves `github:`, `livekit:` and `users:` unset and says so in full: with them unset the
application starts onto its settings alone, because every session service is assembled behind the
session-user resolver that `github:` produces. Configuring an identity is
[2026-09-18-desktop-install-configures-no-identity.md](../../../../docs/dev/todo/2026-09-18-desktop-install-configures-no-identity.md)'s
work, deliberately not this change's.

Docs: [config-resolution-and-install.md](../config-resolution-and-install.md),
[../../README.md](../../README.md),
[tddy-desktop-tauri.md](../../../../docs/ft/desktop/tddy-desktop-tauri.md).
