# Systemd install (`./install --systemd`)

The repo root **`./install`** script installs **`tddy-daemon`**, **`tddy-coder`**, **`tddy-tools`**, and the native **`codex-acp`** binary (from **`@zed-industries/codex-acp`** after **`./dev bun install`**) as a systemd service: copies release binaries, installs **`codex-acp`** into the same bin directory, installs a production config template when missing, writes **`tddy-daemon.service`**, copies the **tddy-web** static bundle when present, and runs **`systemctl`** enable/start (unless disabled for tests).

## Usage

```bash
sudo ./install --systemd           # install from existing ./target/release binaries
sudo ./install --systemd --build # run ./release first, then install
```

- Requires **root** unless **`INSTALL_NO_SYSTEMCTL=1`** (test/CI harness).
- Release binaries must exist under **`target/release/`** (use **`--build`** or run **`./release`** first).
- Web dashboard: build **`packages/tddy-web`** (`bun run build`) so **`packages/tddy-web/dist`** exists before install if you want the bundle copied.
- **Codex ACP:** run **`./dev bun install`** from the repo root so **`node_modules/@zed-industries/codex-acp-<os>-<arch>/bin/codex-acp`** exists; **`./install`** copies it to **`$INSTALL_BIN_DIR/codex-acp`**.

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
| `INSTALL_DAEMON_USER` | Service user the generated unit runs as (default **`tddy`**). Set **`INSTALL_DAEMON_USER=root`** to restore the multi-user setuid spawning mode (`Delegate=`/`AppArmorProfile=` become unnecessary). |
| `INSTALL_DAEMON_GROUP` | Service group (default: same as `INSTALL_DAEMON_USER`). |
| `INSTALL_APPARMOR_DIR` | Directory the `tddy-daemon` AppArmor profile is written to (default **`/etc/apparmor.d`**). |

## Behavior notes

- **Binaries** **`tddy-*`** are copied from **`target/release/`** (overwritten on each install). **`codex-acp`** is copied from **`node_modules/.../bin/codex-acp`** (same **`INSTALL_BIN_DIR`**).
- **Config** is skipped if **`daemon.yaml`** already exists.
- **Unit file** behavior depends on **`INSTALL_OVERWRITE_SYSTEMD_UNIT`** (see above).
- **`systemctl daemon-reload`**, **enable**, and **start** run after files are installed when **`INSTALL_NO_SYSTEMCTL` is unset.

## Unprivileged service (Linux cgroups sandbox)

By default the generated unit runs the daemon as the unprivileged user **`tddy`** rather than root, and provisions the two grants the [Linux cgroups sandbox](../../../packages/tddy-sandbox/docs/architecture.md#linux-cgroups-jail) needs to work without root:

- **Unit flip** — the template emits **`User=tddy`** / **`Group=tddy`** / **`Delegate=yes`** (a writable cgroup v2 subtree for per-session scopes with memory/cpu/pids limits) / **`AppArmorProfile=tddy-daemon`** (grants the daemon binary unprivileged user namespaces on hosts where `apparmor_restrict_unprivileged_userns=1`, e.g. Ubuntu 24.04).
- **Service user** — `./install` creates the (configurable) system user/group when missing (`useradd --system`).
- **Directory ownership** — the daemon log and auth-storage dirs are `chown`ed to the service user/group so the unprivileged process can write them.
- **AppArmor profile** — the profile is rendered from `packages/tddy-daemon/apparmor/tddy-daemon` (binary path substituted), written to `INSTALL_APPARMOR_DIR`, and loaded with `apparmor_parser -r` **before** the service starts (the `AppArmorProfile=` transition fails the unit if the profile is absent; a missing `apparmor_parser` is a warning, not a hard error).
- **Runtime cgroup base** — nothing is hardcoded: the daemon derives its delegated cgroup v2 base from `/proc/self/cgroup` at runtime, overridable via the optional commented `sandbox_cgroup:` block in `daemon.yaml.production`.
- **Restore root mode** — `INSTALL_DAEMON_USER=root` returns to multi-user setuid spawning; `Delegate=`/`AppArmorProfile=` are then unnecessary.

Because the unit is only overwritten when `INSTALL_OVERWRITE_SYSTEMD_UNIT=1`, upgrading an existing root install to the unprivileged default requires that flag (or a manual edit of the installed unit).

## Verification and tests

- Automated: **`packages/tddy-e2e`** — **`install_contract`** (static checks) and **`tests/install_script.rs`** (temp tree, **`INSTALL_NO_SYSTEMCTL=1`**).
- **Operator smoke (optional):** run **`sudo ./install --systemd`** on a target host and confirm **`systemctl status tddy-daemon`** before production rollout.

## Related

- Root **[AGENTS.md](../../../AGENTS.md)** — scripts table and install overview.
- **[changelog](changelog.md)** — daemon product changelog.
