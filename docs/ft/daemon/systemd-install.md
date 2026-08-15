# Systemd install (`./install --systemd`)

The repo root **`./install`** script installs **`tddy-supervisor`**, **`tddy-daemon`**, **`tddy-coder`**, **`tddy-tools`**, and the native **`codex-acp`** binary (from **`@zed-industries/codex-acp`** after **`./dev bun install`**) as a systemd service: copies release binaries, installs **`codex-acp`** into the same bin directory, installs the production config templates when missing, writes **`tddy-supervisor.service`** and **`tddy-supervisor.socket`**, copies the **tddy-web** static bundle when present, and runs **`systemctl`** enable/start (unless disabled for tests).

**The installed unit is the supervisor, not the daemon.** **`tddy-supervisor`** is the only systemd unit and the only root process: it starts **`tddy-daemon`** as an unprivileged managed child declared in **`supervisor.yaml`**, and brokers the operations that need privilege (cgroup v2 scopes, spawning sessions as other OS users, sandbox namespace/mount setup). No **`tddy-daemon.service`** is installed — a second unit would start the daemon twice — and an inherited **`tddy-daemon.socket`** and **`tddy-daemon.service`** are stopped, disabled and **masked** on upgrade (socket first; see [Behavior notes](#behavior-notes)). For a deployment with no supervisor, see **[docs/dev/tddy-daemon.service.example](../../dev/tddy-daemon.service.example)**.

## Usage

```bash
sudo ./install --systemd           # install from existing ./target/release binaries
sudo ./install --systemd --build # run ./release first, then install
sudo ./install --systemd --update-systemd-unit # also rewrite the unit files from this script's templates
```

- Requires **root** unless **`INSTALL_NO_SYSTEMCTL=1`** (test/CI harness).
- Release binaries must exist under **`target/release/`** (use **`--build`** or run **`./release`** first).
- Web dashboard: build **`packages/tddy-web`** (`bun run build`) so **`packages/tddy-web/dist`** exists before install if you want the bundle copied.
- **Codex ACP:** run **`./dev bun install`** from the repo root so **`node_modules/@zed-industries/codex-acp-<os>-<arch>/bin/codex-acp`** exists; **`./install`** copies it to **`$INSTALL_BIN_DIR/codex-acp`**.

### Flags

| Flag | Purpose |
|------|---------|
| `--systemd` | Required. Install binaries, config templates and unit files; enable+start the service. |
| `--build` | Run `./release` before copying binaries. |
| `--user` | Per-user install (`systemctl --user`, XDG paths, no supervisor, no root). |
| `--headless` | Do not require or ship the **tddy-web** bundle (daemon still serves `/rpc` + `/api/config`). |
| `--update-systemd-unit` | Replace an existing unit file; default preserves an existing file so local edits (e.g. **Delegate=**) are kept. A host installed before the supervisor existed has no `tddy-supervisor.*` unit, so it gets one without this flag — it is only needed to pull template changes (e.g. `Conflicts=`, `ListenStream=`) into a host that already has them. |

## Paths and defaults

| Artifact | Default | Override |
|----------|---------|----------|
| Binaries | `$INSTALL_PREFIX/bin` | `INSTALL_BIN_DIR` or `INSTALL_PREFIX` |
| Supervisor config | `$INSTALL_CONFIG_DIR/supervisor.yaml` | `INSTALL_CONFIG_DIR` (default `/etc/tddy`) |
| Daemon config | `$INSTALL_CONFIG_DIR/daemon.yaml` | `INSTALL_CONFIG_DIR` (default `/etc/tddy`) |
| Unit file | `$INSTALL_SYSTEMD_DIR/tddy-supervisor.service` | `INSTALL_SYSTEMD_DIR` |
| Socket unit | `$INSTALL_SYSTEMD_DIR/tddy-supervisor.socket` | `INSTALL_SYSTEMD_DIR` |
| Supervisor socket | `/run/tddy-supervisor.sock` (root:`tddy-clients`, 0660) | `INSTALL_SUPERVISOR_SOCKET_PATH` |
| Daemon client socket | `/run/tddy-daemon.sock` (root:`tddy-clients`, 0660) | `INSTALL_DAEMON_SOCKET_PATH` |
| Web static files | `$INSTALL_PREFIX/share/tddy/web` | `INSTALL_WEB_BUNDLE_DIR` |

Both configs are installed from their **`*.yaml.production`** templates only when the target file is absent (existing config is never overwritten). Every generated file — both configs, both unit files and the AppArmor profile — is rendered to a temporary file next to its destination and moved into place only once the whole content exists, so a failed render leaves the previous file untouched instead of a 0-byte one that the "never overwrite" rule would then preserve forever. Placeholder values are substituted literally (no **`sed`**): a path containing **`#`** or **`&`** renders verbatim, and a value spanning more than one line is refused before anything is written. An operator **`daemon.yaml`** lists **`allowed_tools`** and **`allowed_agents`** so the web connection screen receives tool and backend options from the daemon over RPC (see **`ListTools`** / **`ListAgents`** in [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md)). Optional **`telegram`** (bot token, chat ids, enabled flag) for session status notifications is documented in [telegram-notifications.md](telegram-notifications.md).

**`supervisor.yaml`** is the whole privilege surface of the host and is installed with **`spawn_policy`** and the **`cgroup`** ceilings deliberately empty: an operator who declares no policy grants no privilege, so every request to spawn a session as another OS user is denied until those lists are filled in. The rendered template documents each field inline. Both configs and the config directory are root-owned and never writable by group or other — re-asserted on every run, including for a file that was preserved, so a host installed under a permissive **`umask`** is repaired by installing again.

**`daemon.yaml`** carries the matching **`supervisor.socket_path`**. Its presence is the switch: session spawning and repo cloning are delegated over that socket, and an unreachable or refusing supervisor **fails the operation** rather than falling back to spawning as the daemon user. ⚠️ **Sandbox, claude-cli, cursor-cli and PTY sessions still spawn from the daemon** and therefore still run as the daemon user on a supervised host — see the tddy-supervisor follow-ups in **`docs/dev/TODO.md`**.

The generated unit uses **`ExecStart`** pointing at the resolved **`tddy-supervisor`** binary and **`supervisor.yaml`** path. For a commented manual template, see **[docs/dev/tddy-supervisor.service.example](../../dev/tddy-supervisor.service.example)**.

## Environment variables

| Variable | Purpose |
|----------|---------|
| `INSTALL_PREFIX` | Base prefix (default `/usr/local`). |
| `INSTALL_BIN_DIR` | Binary destination (default `$INSTALL_PREFIX/bin`). |
| `INSTALL_CONFIG_DIR` | Config directory (default `/etc/tddy`). |
| `INSTALL_SYSTEMD_DIR` | systemd unit directory (default `/etc/systemd/system`). |
| `INSTALL_WEB_BUNDLE_DIR` | Web bundle directory (default `$INSTALL_PREFIX/share/tddy/web`). |
| `INSTALL_SUPERVISOR_SOCKET_PATH` | Where **`tddy-supervisor.socket`** creates the privileged RPC socket (default **`/run/tddy-supervisor.sock`**). Written into `supervisor.yaml`, `daemon.yaml` and the socket unit, so all three always agree. |
| `INSTALL_DAEMON_SOCKET_PATH` | Where the supervisor binds the **daemon's** client-facing ConnectionService socket (default **`/run/tddy-daemon.sock`**). Substituted into both the `socket:` block of the `tddy-daemon` service in `supervisor.yaml` and `local.socket_path` in `daemon.yaml`, so the two cannot drift. The supervisor binds it as root before the daemon starts and passes the listener as fd 3 — replacing what `tddy-daemon.socket` used to do, since an unprivileged child cannot bind in `/run`. |
| `INSTALL_NO_SYSTEMCTL=1` | Skip the root check and every action needing root: all **`systemctl`** calls (reload, enable/restart, masking the legacy units, the *is-active* verification), **`groupadd`/`useradd`/`usermod`**, the daemon log + state directories and their **`chown`**, and the AppArmor profile. ⚠️ It **redirects nothing**: binaries, the web bundle, both configs and both unit files are still written to the four `INSTALL_*_DIR` destinations, so a test install must override all four as well. |
| `INSTALL_DAEMON_USER` | Unprivileged user the daemon runs as, declared in `supervisor.yaml` (default **`tddy`**). **`root`** is rejected: the supervisor refuses a root-run managed service at config load, and multi-user session spawning is now the supervisor's job (`spawn_policy.allowed_session_users`). |
| `INSTALL_DAEMON_GROUP` | Service group (default: same as `INSTALL_DAEMON_USER`). |
| `INSTALL_SOCKET_GROUP` | Group granted access to the supervisor socket (default **`tddy-clients`**); the daemon user is added to it. Membership only gets a client as far as `connect()` — each request is authorized by `SO_PEERCRED`. |
| `INSTALL_APPARMOR_DIR` | Directory the `tddy-daemon` AppArmor profile is written to (default **`/etc/apparmor.d`**). |

## Behavior notes

- **Binaries** **`tddy-*`** are copied from **`target/release/`** (overwritten on each install). **`codex-acp`** is copied from **`node_modules/.../bin/codex-acp`** (same **`INSTALL_BIN_DIR`**).
- **Config** is skipped if **`supervisor.yaml`** / **`daemon.yaml`** already exists (each independently). Because substitution only happens for a file this run *creates*, changing **`INSTALL_DAEMON_SOCKET_PATH`** or **`INSTALL_SUPERVISOR_SOCKET_PATH`** on a later run rewrites nothing — install re-reads the files it kept and **warns** for every one that does not name the resolved path (it cannot fix them without overwriting operator config).
- **Unit file** behavior depends on **`--update-systemd-unit`** (see [Flags](#flags)), for both the service and the socket unit. The AppArmor profile is guarded the same way: an existing one is kept, so operator edits survive a reinstall.
- **Legacy daemon unit** — an inherited **`tddy-daemon.socket`** and then **`tddy-daemon.service`** are stopped, disabled and **masked**. The order matters: stopping the service does not stop the socket it is activated from, and that socket listens on the very path the supervisor is about to bind, so a `connect()` in between would re-launch the legacy daemon onto the daemon's web port and token storage. Masking (not just `disable`) is what survives a `systemctl preset-all` or a package upgrade re-adding the symlink. `./install` never deletes an operator's files, so when the legacy unit file *is* `$INSTALL_SYSTEMD_DIR/tddy-daemon.service` the mask cannot be created there — install reports that with the manual remedy (move the file aside) instead of forcing it. The supervisor unit also carries **`Conflicts=tddy-daemon.service tddy-daemon.socket`** as a backstop.
- **`systemctl daemon-reload`**, **enable**, and **start** run after files are installed when **`INSTALL_NO_SYSTEMCTL`** is unset. The socket unit is started first so the listening fd exists before the supervisor adopts it.
- **Start is verified, not assumed** — `Type=simple` makes `systemctl restart` return as soon as fork/exec succeeds, so a supervisor that rejects its config and exits a millisecond later would look healthy. Install samples `systemctl is-active tddy-supervisor` five times a second apart and **fails** if it is ever not `active` (a crash loop reports `activating`/`failed`), dumping `systemctl status` and the last journal lines. This matters most on upgrade: the legacy unit is already disarmed by then, so a crash loop means nothing is serving.
- **AppArmor load is best-effort** — a missing **or failing** `apparmor_parser -r` (a host with no AppArmor LSM) is a warning; it must not abort an otherwise complete install before the units are enabled.
- **Daemon state** — both `/var/log/tddy-daemon` and the `auth_storage` directory `/var/lib/tddy/auth` itself are created and chowned to the service user (the `auth` directory at mode `0700`). The daemon refuses to start when `auth_storage` is unwritable, so creating only its parent left a fresh host crash-looping.

## One root process (privilege boundary)

The unit runs **`tddy-supervisor`** as root; everything network-facing runs unprivileged:

- **Unit** — **`User=root`** / **`Group=root`** / **`Delegate=yes`** (a writable cgroup v2 subtree the supervisor carves per-session scopes out of) / **`KillMode=mixed`** + **`TimeoutStopSec=30`** (the supervisor gets SIGTERM alone so it can forward it to its children within `shutdown_grace_secs`; survivors are SIGKILLed cgroup-wide, so no session outlives the unit).
- **Daemon as a child** — **`supervisor.yaml`** declares `tddy-daemon` with **`user: tddy`**, its config path, and a restart policy (backoff, retry ceiling, stability threshold). The daemon needs no `Delegate=` and no AppArmor userns grant of its own; the supervisor owns both.
- **Service user** — `./install` creates the (configurable) system user/group when missing (`useradd --system`) and adds it to `INSTALL_SOCKET_GROUP` so the daemon can open the supervisor socket.
- **Directory ownership** — the daemon log dir, the state dir and the `auth_storage` dir are `chown`ed to the service user/group so the unprivileged process can write them; the config dir, the unit dir and everything install generates in them stay root-owned and not group/other-writable (a writable `supervisor.yaml` is the policy the root broker enforces; a writable unit is an `ExecStart=` anyone can rewrite). The web bundle is copied with `--no-preserve=ownership` so the *build* user does not end up owning the JavaScript the daemon serves.
- **Socket** — **`tddy-supervisor.socket`** creates the RPC socket as root with group access at mode 0660 and passes the fd, so nothing binds in `/run` at runtime. Peer credentials (`SO_PEERCRED`) authorize every request against the uids of declared services.
- **Privilege-drop ordering** — for a sandbox the supervisor drops to the target uid/gid **before** `unshare(CLONE_NEWUSER)`, so the user namespace is owned by the unprivileged target user exactly as on an unsupervised host. Root is used only to choose the uid and write the cgroup scope.
- **AppArmor profile** — still rendered from `packages/tddy-daemon/apparmor/tddy-daemon` (binary path substituted), written to `INSTALL_APPARMOR_DIR` **when absent** (an existing profile is kept) and loaded with `apparmor_parser -r`. No installed unit references `AppArmorProfile=` anymore: the profile attaches by binary path under `flags=(unconfined)`, grants only `userns`, and covers the jails the daemon still builds itself. A missing **or failing** `apparmor_parser` is a warning, not a hard error.
- **Runtime cgroup base** — nothing is hardcoded: the base is derived from `/proc/self/cgroup` at runtime (the unit's `Delegate=yes` subtree), overridable via the commented `cgroup.base_override` in `supervisor.yaml.production`.

Because a unit is only overwritten when `--update-systemd-unit` is passed, a host that already has a hand-edited `tddy-supervisor.service`/`.socket` keeps it as-is and needs that flag to pick up template changes. A host installed *before* the supervisor existed has no such file, so it gets both units on the first supervised install; its `tddy-daemon.service` is a different file name and is disarmed rather than replaced.

## Verification and tests

- Automated: **`packages/tddy-e2e`** — **`install_contract`** (static checks, including the ones `INSTALL_NO_SYSTEMCTL=1` cannot reach: legacy-unit disarm order and masking, the *is-active* verification, the AppArmor guard, and that no generated file is written by a truncating redirect), **`tests/install_script.rs`** and **`tests/install_supervisor.rs`** (temp tree, **`INSTALL_NO_SYSTEMCTL=1`** — placeholder rendering including `#`/`&` values, socket-path agreement, file modes, preserve behavior).
- **Operator smoke (optional):** run **`sudo ./install --systemd`** on a target host and confirm **`systemctl status tddy-supervisor`** shows the unit as root with `tddy-daemon` as an unprivileged child, before production rollout.

## Related

- Root **[AGENTS.md](../../../AGENTS.md)** — scripts table and install overview.
- **[changelog](changelog.md)** — daemon product changelog.
