# tddy-remote client (remote sandbox)

## Purpose

**`tddy-remote`** is the CLI for **Connect** (and LiveKit-capable) access to **`RemoteSandboxService`**: list configured authorities, run remote commands, optional PTY shell, and **rsync `--rsh`** bridging.

## Configuration

YAML file (path from **`--config`** or **`TDDY_REMOTE_CONFIG`**):

- **`authorities`:** Each entry has **`id`** and **`connect_base_url`** (daemon base URL, no trailing slash required in logic).
- **`default_authority`:** Optional. Used when the host argument is empty, and as a fallback Connect target when the host string does not match any **`id`**.

## Commands

| Command | Behavior |
|---------|----------|
| **`authorities list`** | Prints authority **`id`** lines from the config file. |
| **`exec <HOST> …`** | Unary **`ExecNonInteractive`**; remote exit code maps to process exit code. |
| **`shell [--pty] <HOST>`** | Shell session over Connect (PTY optional). |
| **RSH / external** | **`rsync`** invokes **`tddy-remote`** as **`RSYNC_RSH`**; see **`rsh.rs`** for **`poll(2)`**-based byte bridging between stdio and the TCP session from **`OpenRsyncSession`**. |

## Environment

- **`TDDY_REMOTE_RSYNC_SESSION`:** Logical sandbox session id reused across put/exec/rsync steps in tests and automation.

## Related

- Feature: [docs/ft/daemon/remote-sandbox.md](../../../docs/ft/daemon/remote-sandbox.md)
- Daemon: [packages/tddy-daemon/docs/remote-sandbox.md](../../tddy-daemon/docs/remote-sandbox.md)
