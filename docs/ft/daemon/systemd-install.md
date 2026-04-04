# Systemd install (`./install --systemd`)

The repo root **`./install`** script installs **`tddy-daemon`**, **`tddy-coder`**, and **`tddy-tools`** as a systemd service: copies release binaries, installs a production config template when missing, writes **`tddy-daemon.service`**, copies the **tddy-web** static bundle when present, and runs **`systemctl`** enable/start (unless disabled for tests).

## Usage

```bash
sudo ./install --systemd           # install from existing ./target/release binaries
sudo ./install --systemd --build # run ./release first, then install
```

- Requires **root** unless **`INSTALL_NO_SYSTEMCTL=1`** (test/CI harness).
- Release binaries must exist under **`target/release/`** (use **`--build`** or run **`./release`** first).
- Web dashboard: build **`packages/tddy-web`** (`bun run build`) so **`packages/tddy-web/dist`** exists before install if you want the bundle copied.

## Paths and defaults

| Artifact | Default | Override |
|----------|---------|----------|
| Binaries | `$INSTALL_PREFIX/bin` | `INSTALL_BIN_DIR` or `INSTALL_PREFIX` |
| Config | `$INSTALL_CONFIG_DIR/daemon.yaml` | `INSTALL_CONFIG_DIR` (default `/etc/tddy`) |
| Unit file | `$INSTALL_SYSTEMD_DIR/tddy-daemon.service` | `INSTALL_SYSTEMD_DIR` |
| Web static files | `$INSTALL_PREFIX/share/tddy/web` | `INSTALL_WEB_BUNDLE_DIR` |

Production config is installed from **`daemon.yaml.production`** only when **`daemon.yaml`** is absent (existing config is never overwritten). An operator **`daemon.yaml`** lists **`allowed_tools`** and **`allowed_agents`** so the web connection screen receives tool and backend options from the daemon over RPC (see **`ListTools`** / **`ListAgents`** in [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md)). Optional **`telegram`** (bot token, chat ids, enabled flag) for session status notifications is documented in [telegram-notifications.md](telegram-notifications.md).

The generated unit uses **`ExecStart`** pointing at the resolved **`tddy-daemon`** binary and config path. For a commented manual template, see **[docs/dev/tddy-daemon.service.example](../../../dev/tddy-daemon.service.example)**.

## Environment variables

| Variable | Purpose |
|----------|---------|
| `INSTALL_PREFIX` | Base prefix (default `/usr/local`). |
| `INSTALL_BIN_DIR` | Binary destination (default `$INSTALL_PREFIX/bin`). |
| `INSTALL_CONFIG_DIR` | Config directory (default `/etc/tddy`). |
| `INSTALL_SYSTEMD_DIR` | systemd unit directory (default `/etc/systemd/system`). |
| `INSTALL_WEB_BUNDLE_DIR` | Web bundle directory (default `$INSTALL_PREFIX/share/tddy/web`). |
| `INSTALL_NO_SYSTEMCTL=1` | Skip root check and all **`systemctl`** calls (automated tests). |
| `INSTALL_OVERWRITE_SYSTEMD_UNIT=1` | Replace an existing unit file; default preserves an existing file so local edits (e.g. **User=**) are kept. |

## Behavior notes

- **Binaries** are copied from **`target/release/`** (overwritten on each install).
- **Config** is skipped if **`daemon.yaml`** already exists.
- **Unit file** behavior depends on **`INSTALL_OVERWRITE_SYSTEMD_UNIT`** (see above).
- **`systemctl daemon-reload`**, **enable**, and **start** run after files are installed when **`INSTALL_NO_SYSTEMCTL` is unset.

## Verification and tests

- Automated: **`packages/tddy-e2e`** — **`install_contract`** (static checks) and **`tests/install_script.rs`** (temp tree, **`INSTALL_NO_SYSTEMCTL=1`**).
- **Operator smoke (optional):** run **`sudo ./install --systemd`** on a target host and confirm **`systemctl status tddy-daemon`** before production rollout.

## Related

- Root **[AGENTS.md](../../../AGENTS.md)** — scripts table and install overview.
- **[changelog](changelog.md)** — daemon product changelog.
