# 2026-09-18 — `./install --desktop`

**Type:** Feature

A local install of Tddy Desktop, and the release-build configuration rule that makes one possible.

`./install --desktop` is a **different deployment** from `--systemd`, not a variant of it: the
application's own process *is* a `tddy-daemon`, so the install writes no unit, creates no service
user, deploys no web bundle directory, serves no port, and ships neither `tddy-daemon` nor
`tddy-supervisor`. Passing both flags in one run is an error — two daemons on one machine would
contend for the same data directory and the same LiveKit identity. macOS gets `Tddy Desktop.app` in
`~/Applications` and a `tddy-desktop` launcher on `PATH`; Linux gets the binary, a `.desktop` entry
and a hicolor icon under `$XDG_DATA_HOME`.

`packages/tddy-desktop/src-tauri/src/config_source.rs` previously resolved the daemon config only
from a checkout, so an installed application could not start at all. Resolution now splits on the
build profile: debug keeps every development rule, release reads `~/.tddy/desktop.yaml` and nothing
else. See [the package changeset](../../../packages/tddy-desktop/docs/changesets/2026-09-18-install-desktop.md).

`desktop.yaml.production` is the template it renders, and an existing config is never overwritten.
It declares `listen.web_port` — required by `runtime::build`, and in this deployment the loopback
port a GitHub sign-in returns on rather than a served listener — and no `web_bundle_path`, since the
dashboard is embedded at build time. It leaves `github:`, `livekit:` and `users:` unset, which is an
application that starts onto its settings and offers no sessions; the template says so at length, and
[2026-09-18-desktop-install-configures-no-identity.md](../todo/2026-09-18-desktop-install-configures-no-identity.md)
carries the work of configuring an identity.

`--build` runs `./release`, the `tddy-web` bundle and `tauri build` in that order, because the
application embeds the bundle. New overrides: `INSTALL_TDDY_HOME`, `INSTALL_DESKTOP_APP_DIR`,
`INSTALL_XDG_DATA_DIR`. No dependency was added. `packages/tddy-e2e` covers the flag contract, the
template's required and absent keys and placeholder set, and a full install into a redirected `HOME`
on both platforms.

Root scripts table: [AGENTS.md](../../../AGENTS.md). Feature doc:
[tddy-desktop-tauri.md](../../ft/desktop/tddy-desktop-tauri.md).
