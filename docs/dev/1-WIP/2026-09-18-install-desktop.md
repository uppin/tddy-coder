# 2026-09-18 — `./install --desktop`

**Type:** Feature

`./install --desktop` installs Tddy Desktop locally: the application goes to `~/Applications` on
macOS (`Tddy Desktop.app`) or to `$BIN_DIR` on Linux (with a `.desktop` entry and a hicolor icon
under `$XDG_DATA_HOME`), `tddy-desktop` resolves by name on `PATH` on both, and the daemon's
configuration is rendered to `~/.tddy/desktop.yaml`. It is a *different deployment* from `--systemd`,
not a variant of it: the application's own process **is** a `tddy-daemon`, so the install ships no
unit, no service user, no web bundle directory, no listening port — and no `tddy-daemon` or
`tddy-supervisor` binary. Passing both flags in one run is an error, because two daemons on one
machine contend for the same data directory and the same LiveKit identity.

**A release build now reads exactly one file.** `packages/tddy-desktop/src-tauri/src/config_source.rs`
previously resolved the daemon config only from a checkout — `TDDY_DAEMON_CONFIG`, then
`dev.desktop.yaml` found by walking up from the working directory. An installed `.app` launched from
the Dock has `/` for a working directory and no checkout above it, so an installed application could
not start at all. Resolution now splits on the build profile: a debug build keeps every development
rule unchanged; a release build reads `~/.tddy/desktop.yaml` and nothing else, and names
`./install --desktop` when that file is absent. There is no fallback in either direction — anything a
Dock launch found by walking up it would have found by accident.

`desktop.yaml.production` is the new template. It declares no `listen:` and no `web_bundle_path:` on
purpose (the dashboard reaches the daemon over the webview's IPC bridge, and the bundle is embedded
at build time), no `github:` block so a single-operator install opens straight onto its sessions, a
file log under `~/.tddy/logs` because a Dock launch has no terminal, `auth_storage` created mode
0700, and `allowed_tools` pointing at the absolute paths this run installed `tddy-coder` and
`tddy-tools` to. An existing `desktop.yaml` is never overwritten by a reinstall.

On macOS the CLI binaries are installed **twice**, deliberately: inside `Contents/MacOS` so the
daemon's sibling-of-`current_exe()` lookup finds `tddy-sandbox-runner` and `tddy-index-daemon`, and
in `$BIN_DIR` so `allowed_tools` and a hand-run `tddy-tools` resolve. `$BIN_DIR/tddy-desktop` is a
launcher script that `exec`s the binary inside the bundle rather than a symlink: a symlink would make
the process's executable path `$BIN_DIR/tddy-desktop` and break both bundle identity and that sibling
lookup. On Linux one copy serves both roles.

`--build` runs three builds in order — `./release`, then the `tddy-web` bundle, then `tauri build` —
because the application *embeds* the bundle; building the app first produces a stale dashboard and no
error. Without `--build` the install preflights for the artifacts and fails naming the exact command.

New overrides: `INSTALL_TDDY_HOME`, `INSTALL_DESKTOP_APP_DIR`, `INSTALL_XDG_DATA_DIR`;
`INSTALL_DAEMON_LOG_DIR` and `INSTALL_AUTH_STORAGE_DIR` keep their names with desktop defaults.
Relocating `INSTALL_TDDY_HOME` away from `$HOME/.tddy` warns, since a release build has no second
place to look.

No dependency was added. `packages/tddy-e2e` covers the flag contract, the template's absent keys and
placeholder set, and a full install into a redirected `HOME` on both platforms.
