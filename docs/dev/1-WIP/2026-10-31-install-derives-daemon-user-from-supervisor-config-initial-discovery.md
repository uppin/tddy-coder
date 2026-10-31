# Initial Discovery: Install derives the daemon user from supervisor.yaml

**Changeset**: [2026-10-31-install-derives-daemon-user-from-supervisor-config.md](./2026-10-31-install-derives-daemon-user-from-supervisor-config.md)
**Date**: 2026-10-31
**Passes**: 2

## Combined Conclusions

State A: `./install` holds a **second source of truth** for the daemon's OS identity next to
`supervisor.yaml`. `INSTALL_DAEMON_USER`/`INSTALL_DAEMON_GROUP` (default `tddy`) drive account
creation, socket-group membership and the `chown -R` of the log + auth dirs — and are *rendered*
into `supervisor.yaml` only when that file does not exist yet. On an upgrade the env vars act with
no relation to the `user:`/`group:` the preserved config declares, so an operator who changed
`supervisor.yaml`'s `user:` (or chowned the dirs by hand) gets a daemon that spawns as one user
against directories owned by another. Nothing in the runtime ever re-chowns: the supervisor only
chowns sockets it hands to the daemon under `/run`, the unit has no `ExecStartPre`, there is no
`tmpfiles.d` entry, and the `.deb` maintainer scripts create no user. A restart therefore cannot
reset ownership — the drift is created (or healed) only by an install run.

Decision (developer, this session): `daemon.yaml` **cannot** own its own uid — the spawning parent
chooses the user before the process exists and the daemon is unprivileged by design — so the single
source becomes **`supervisor.yaml`'s `tddy-daemon` service `user:`/`group:`**. `INSTALL_DAEMON_USER`
survives only as the seed for a *fresh* install (it must be rendered into the config somehow); on an
upgrade the existing config is authoritative and a disagreeing env var is an error, not a warning.

Constraints found:

- The account/chown branch (`install` ~1075–1101) is skipped entirely under
  `INSTALL_NO_SYSTEMCTL=1`, so the e2e harness cannot observe `useradd`/`chown` directly. The
  derivation decision must therefore run **outside** that guard and be observable through narration
  and refusal behavior; the real privileged actions stay covered by the VM-backed suites.
- The script has no YAML parser. It already does targeted, needle-based agreement checks
  (`warn_unless_file_declares`, line ~656) and refuses to overwrite operator config. The extraction
  must follow that idiom: find the `- name: tddy-daemon` service block, take its `user:`/`group:`,
  and fail loudly when either is missing — never fall back to `tddy` silently.
- The supervisor rejects a root-run managed service at config load; install mirrors that today for
  the env var (`install`:920). The same rejection must apply to a user *derived from the config*.
- `supervisor.yaml.production` always renders both `user:` and `group:` (`__DAEMON_USER__`,
  `__DAEMON_GROUP__`), but the supervisor treats `group` as optional. For install's account work a
  group is required (useradd `--gid`, chown target), so a config missing `group:` fails loudly with
  the remedy rather than defaulting.

Open question carried into implementation: whether a VM-backed test already installs inside a guest
and can own the real `useradd`/`chown` assertions (`packages/tddy-vm` references `./install`); if
yes, extend it, if no, note the gap in the changeset rather than inventing a rootful CI path.

## Exploration 1: Where the daemon user is defined and used — 2026-10-31

**Agent**: parent Grep/Glob/Read
**Scope**: root `install` script, `supervisor.yaml.production`, supervisor config code — establish
every place the daemon's OS identity is set or consumed.

### Sequence

1. Grep `chown|tddy:tddy|useradd|usermod` repo-wide — find every ownership-setting site.
2. Grep `\$DAEMON_USER|\$DAEMON_GROUP|__DAEMON_USER__|__DAEMON_GROUP__` in `install` — all uses of
   the env-derived identity.
3. Read `install` 1060–1130 and 1200–1310 — the account branch, config rendering, agreement checks.
4. Read `install` 260–340 — how `AUTH_STORAGE_PATH`, config paths and the env defaults are resolved.
5. Read `supervisor.yaml.production` in full — the service block shape and the privilege-surface
   framing.
6. Grep `chown` in `packages/**/*.rs` — confirm no runtime code chowns these paths.

### Grep / glob

| Tool | Pattern / glob | Path scope | Notable hits |
|------|----------------|------------|--------------|
| Grep | `chown\|tddy:tddy\|useradd\|usermod` | repo | `install:644,1087,1095,1099,1405`; `tddy-supervisor` socket-only chowns (`service_slot.rs:130`, `server/listener.rs:288`, `host_socket.rs`); `tddy-spawn/spawner.rs:795` (session files); VM testkit recipes |
| Grep | `\$DAEMON_USER\|\$DAEMON_GROUP\|__DAEMON_USER__\|__DAEMON_GROUP__` | `install` | 325–326 (env defaults), 920 (root rejection), 1083–1099 (groupadd/useradd/usermod/chown), 1163–1164 (rendering) |
| Grep | `\buser[:= ]+["']?tddy\b\|chown .*tddy\|useradd .*tddy` | scripts, yaml, sh, md | only `publish.sh:48,50` (package name/maintainer, not a user) |
| Grep | `chown` | `packages/**/*.rs` | no host-runtime chown of log/auth dirs; `tddy-agent-launch/host_session_socket.rs:339` chowns host-session sockets only |

### Inspected files

#### `install` (lines 1075–1101)

**Why**: the account-creation and ownership branch — the code that must switch from env-derived to
config-derived identity.
**Excerpt**:

```bash
elif [[ "$NO_SYSTEMCTL" != "1" ]]; then
  echo "install: ensuring service user/group ${DAEMON_USER}:${DAEMON_GROUP}"
  if ! getent group "$DAEMON_GROUP" >/dev/null; then
    groupadd --system "$DAEMON_GROUP"
  fi
  if ! id -u "$DAEMON_USER" >/dev/null 2>&1; then
    useradd --system --gid "$DAEMON_GROUP" --home-dir "$AUTH_STORAGE_DIR" \
      --shell /usr/sbin/nologin "$DAEMON_USER"
  fi
  ...
  chown -R "${DAEMON_USER}:${DAEMON_GROUP}" "$DAEMON_LOG_DIR" "$AUTH_STORAGE_DIR"
  chmod 0700 "$AUTH_STORAGE_PATH"
```

The whole branch is inside `elif [[ "$NO_SYSTEMCTL" != "1" ]]` — invisible to the e2e harness.

#### `install` (lines 1151–1168)

**Why**: confirm the config render/preserve split that makes the env var a second source of truth.
**Excerpt**:

```bash
if [[ ! -f "$SUPERVISOR_CONFIG_FILE" ]]; then
  echo "install: creating ${SUPERVISOR_CONFIG_FILE} from supervisor.yaml.production (declares tddy-daemon as user ${DAEMON_USER})"
  render_template "${ROOT_DIR}/supervisor.yaml.production" "$SUPERVISOR_CONFIG_FILE" \
      "__DAEMON_USER__=${DAEMON_USER}" \
      "__DAEMON_GROUP__=${DAEMON_GROUP}"
else
  echo "install: keeping existing supervisor config (not overwriting): ${SUPERVISOR_CONFIG_FILE}"
```

#### `install` (lines 1296–1305) — `warn_unless_file_declares`

**Why**: the existing idiom for config-agreement checking; the new extraction should speak the same
language (explicit failure, named remedy), only stricter (error, not warning — identity drift breaks
the daemon outright, unlike a stale socket path warning).

#### `supervisor.yaml.production` (lines 32–45)

**Why**: the exact block the parser must read; also confirms `user:`/`group:` are per-service.
**Excerpt**:

```yaml
services:
  - name: tddy-daemon
    exec_start: __INSTALL_BIN_DIR__/tddy-daemon
    args: ["-c", "__DAEMON_CONFIG_PATH__"]
    user: __DAEMON_USER__
    group: __DAEMON_GROUP__
    socket:
      path: __DAEMON_SOCKET_PATH__
      group: __SOCKET_GROUP__
      mode: "0660"
```

Note `socket.group` is the *socket-access* group (`tddy-clients`), distinct from the service's
`group:` — the parser must read the service keys, not the socket block's.

#### `packages/tddy-supervisor/src/config.rs` (test around line 663)

**Why**: confirm the supervisor's model of `user`/`group` (group optional).
**Excerpt**: `assert_eq!(daemon.user, "tddy"); assert_eq!(daemon.group, Some("tddy".to_string()));`

### Findings

- The daemon's OS identity has exactly one writer (install) and one consumer (supervisor config),
  but install derives its copy from env vars rather than reading the config back — the drift is
  structural, not a bug in one branch.
- No runtime component (supervisor, daemon, unit files, tmpfiles, deb scripts) chowns the log/auth
  dirs; ownership changes only when `./install` runs. A restart can only *reveal* drift.
- The `socket:` block inside the service also carries a `group:` key — any extraction must be
  scoped to the service's own `user:`/`group:` lines.
- `--user` mode has no supervisor.yaml and runs the daemon as the invoking user; `INSTALL_DAEMON_USER`
  is already unused there (docs state this).

## Exploration 2: Test harness and docs surface — 2026-10-31

**Agent**: parent Grep/Read
**Scope**: `packages/tddy-e2e` install tests, feature doc, TODO backlog — establish how the change
is tested and documented.

### Sequence

1. Read `packages/tddy-e2e/tests/install_supervisor.rs` in full — harness mechanics and existing
   coverage.
2. Grep `INSTALL_DAEMON_USER|INSTALL_NO_SYSTEMCTL` repo-wide — every doc/test referencing the env
   contract.
3. Read `docs/ft/daemon/systemd-install.md` — the doc rows that change meaning.
4. Grep `docs/dev/todo` for `INSTALL_DAEMON_USER|daemon user|chown|useradd` — prerequisite scan.

### Grep / glob

| Tool | Pattern / glob | Path scope | Notable hits |
|------|----------------|------------|--------------|
| Grep | `INSTALL_DAEMON_USER\|INSTALL_NO_SYSTEMCTL` | repo | `install`, `supervisor.yaml.production` (comment), `packages/tddy-vm/src/tddy_host.rs`, `packages/tddy-e2e/tests/install_script.rs`, `install_supervisor.rs`, `src/install_contract.rs`, docs (`systemd-install.md`, `tddy-supervisor.md`, AGENTS/README/CLAUDE) |
| Grep | `INSTALL_DAEMON_USER\|daemon user\|chown\|useradd` | `docs/dev/todo/` | 4 files, none blocking this change (socket-dir read failure diagnostics; host-socket real-run coverage; supervisor gap list; stack-status note quoting the chown) |

### Inspected files

#### `packages/tddy-e2e/tests/install_supervisor.rs`

**Why**: the harness the new acceptance tests will extend.
**Excerpt**:

```rust
fn apply_env(&self, command: &mut Command) {
    command
        .env("INSTALL_NO_SYSTEMCTL", "1")
        .env("INSTALL_BIN_DIR", &self.bin_dir)
        .env("INSTALL_CONFIG_DIR", &self.config_dir)
        .env("INSTALL_SYSTEMD_DIR", &self.systemd_dir)
        .env("INSTALL_WEB_BUNDLE_DIR", &self.web_dir);
}
```

Existing relevant tests: `installs_a_supervisor_config_declaring_the_daemon_as_an_unprivileged_service`
(fresh install renders `user: tddy`), `keeps_an_existing_supervisor_config_rather_than_overwriting_the_policy_in_it`
(preserve behavior), `upgrading_removes_the_supervisor_block_from_an_existing_daemon_yaml_and_says_so`
(narration-asserted upgrade behavior — the pattern to follow for the derived-user narration).
`overwrite_installed_config` is the seam for planting an operator config before an upgrade run.

#### `docs/ft/daemon/systemd-install.md`

**Why**: the operator-facing contract that changes.
**Excerpts**: env-var table rows `INSTALL_DAEMON_USER` ("Unprivileged user the daemon runs as,
declared in `supervisor.yaml` (default `tddy`)") and `INSTALL_DAEMON_GROUP`; Behavior notes
**Daemon state** ("created and chowned to the service user"); One-root-process **Service user** and
**Directory ownership** bullets — all must move from "install decides" to "supervisor.yaml decides,
install obeys".

### Findings

- The e2e harness is functional (real script, temp tree) but rootless; the acceptance tests for this
  change assert the *decision* (derivation source, refusal diagnostics) via exit status, stderr and
  narration — exactly like the existing `upgrading_removes_the_supervisor_block…` test.
- Documentation surface is one feature doc plus the template comments; AGENTS.md/README describe the
  env var only in passing (AGENTS.md scripts table mentions install but not the user vars — verify
  during wrap).
- TODO backlog has no blocking or during entries for this change; the scan is clean.
