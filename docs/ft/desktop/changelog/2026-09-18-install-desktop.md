# 2026-09-18 — Install Tddy Desktop locally

Tddy Desktop installs onto the machine it will run on with `./install --desktop`: the application
into `~/Applications` on macOS or `$BIN_DIR` plus a desktop entry on Linux, `tddy-desktop` resolving
by name on `PATH` on both, and the CLI binaries the embedded daemon spawns beside it. It is a
different deployment from the served `--systemd` install rather than a variant of it — the
application *is* the daemon — so nothing about a service, a unit or a served port is involved, and
asking for both in one run is refused.

An installed application reads `~/.tddy/desktop.yaml` and nothing else. A development run is
unchanged. The installer writes that file on a first install and never overwrites it afterwards, so
everything an operator configures survives a reinstall.

Sessions need an identity — a GitHub block, a session-token secret and a login-to-OS-user mapping —
that the install does not yet configure. Until it is filled in, the application starts and opens on
its settings.
