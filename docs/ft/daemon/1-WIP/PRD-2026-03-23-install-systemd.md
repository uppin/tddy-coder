# PRD: `./install --systemd` — Systemd Service Installer

**Status:** 🚧 Planning
**Created:** 2026-03-23

## Summary

Create an `./install` script at the repo root that installs `tddy-daemon`, `tddy-coder`, and `tddy-tools` as a systemd service. The script copies release binaries, installs a production config template, creates required directories, places the systemd unit file, and enables+starts the service.

For testability, all install paths are overridable via environment variables (`INSTALL_PREFIX`, `INSTALL_BIN_DIR`, `INSTALL_CONFIG_DIR`, `INSTALL_SYSTEMD_DIR`).

## Background

Currently `tddy-daemon` has an example systemd unit file (`docs/dev/tddy-daemon.service.example`) with manual instructions for copying and enabling. The `./release` script builds the binary but there is no automated installer. Setting up the daemon as a systemd service requires multiple manual steps (copy binary, create config dir, place config, write unit file, systemctl enable/start).

An `./install --systemd` script automates this entire flow, making deployment reproducible and testable.

## Affected Features

- [PRD-2026-03-19-tddy-daemon.md](PRD-2026-03-19-tddy-daemon.md) — Daemon deployment model; this PRD adds the automated install path
- [docs/dev/tddy-daemon.service.example](../../../dev/tddy-daemon.service.example) — Existing example unit; installer generates from this or replaces it

## Requirements

### 1. Script Interface

```bash
./install --systemd              # Install as systemd service
./install --systemd --build      # Run ./release first, then install
```

- Script must be run as root (exits with error if not root)
- `--build` flag triggers `./release` before installing

### 2. Installed Artifacts

| Artifact | Default Path | ENV Override |
|----------|-------------|--------------|
| `tddy-daemon` binary | `$PREFIX/bin/tddy-daemon` | `INSTALL_BIN_DIR` |
| `tddy-coder` binary | `$PREFIX/bin/tddy-coder` | `INSTALL_BIN_DIR` |
| `tddy-tools` binary | `$PREFIX/bin/tddy-tools` | `INSTALL_BIN_DIR` |
| Production config | `$CONFIG_DIR/daemon.yaml` | `INSTALL_CONFIG_DIR` |
| Systemd unit file | `$SYSTEMD_DIR/tddy-daemon.service` | `INSTALL_SYSTEMD_DIR` |

**Defaults:**
- `INSTALL_PREFIX`: `/usr/local`
- `INSTALL_BIN_DIR`: `$INSTALL_PREFIX/bin`
- `INSTALL_CONFIG_DIR`: `/etc/tddy`
- `INSTALL_SYSTEMD_DIR`: `/etc/systemd/system`

### 3. Config Handling

- Install a production config template (`daemon.yaml`) to `$CONFIG_DIR/daemon.yaml`
- **Skip** if config already exists (never overwrite user config)
- Production template is distinct from `dev.daemon.yaml` — no stubs, production-appropriate defaults

### 4. Directory Creation

Create required directories:
- `$INSTALL_BIN_DIR` (if not exists)
- `$INSTALL_CONFIG_DIR` (if not exists)
- `$INSTALL_SYSTEMD_DIR` (if not exists)

### 5. Systemd Unit File

Generate a unit file with correct paths based on the resolved `INSTALL_BIN_DIR` and `INSTALL_CONFIG_DIR`:
- `ExecStart` points to `$BIN_DIR/tddy-daemon -c $CONFIG_DIR/daemon.yaml`
- Supersedes the static `docs/dev/tddy-daemon.service.example`

### 6. Service Lifecycle

After installing files, the script:
1. Runs `systemctl daemon-reload`
2. Runs `systemctl enable tddy-daemon`
3. Runs `systemctl start tddy-daemon`

### 7. ENV-Based Path Overrides (Testing)

All paths configurable via ENV for integration testing without touching system directories:

```bash
INSTALL_PREFIX=/tmp/test-install \
INSTALL_CONFIG_DIR=/tmp/test-install/etc \
INSTALL_SYSTEMD_DIR=/tmp/test-install/systemd \
./install --systemd
```

When testing, systemctl commands should be skipped (detected by non-standard `INSTALL_SYSTEMD_DIR`, or a dedicated `INSTALL_NO_SYSTEMCTL=1` flag).

### 8. Idempotency

- Binary copies are always overwritten (update in place)
- Config is never overwritten if it exists
- Unit file is always overwritten (to pick up path changes)
- `systemctl daemon-reload` handles unit file updates

## Success Criteria

1. `sudo ./install --systemd` installs all three binaries, config template, and unit file to correct default paths
2. `systemctl status tddy-daemon` shows the service as enabled and active after install
3. Config at `/etc/tddy/daemon.yaml` is not overwritten on re-install
4. ENV overrides (`INSTALL_PREFIX`, `INSTALL_BIN_DIR`, `INSTALL_CONFIG_DIR`, `INSTALL_SYSTEMD_DIR`) redirect all artifacts
5. `--build` flag triggers `./release` before install
6. Script exits with clear error when not run as root
7. Tests can run the installer in a temp directory without root or systemctl

## Scope Boundaries

**In scope:**
- `./install` script with `--systemd` flag
- Production config template
- Generated systemd unit file
- ENV-based path overrides
- Integration tests using ENV overrides

**Out of scope:**
- Uninstall command (future)
- Package manager integration (deb, rpm)
- User-mode systemd (`--user`) installation
- TLS/certificate setup
