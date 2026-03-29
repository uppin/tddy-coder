# tddy-remote

CLI for **remote sandboxes** on **tddy-daemon**: Connect-based exec, shell, and **rsync `--rsh`** bridging to **`remote_sandbox.v1.RemoteSandboxService`**.

## Quick Start

### Development

```bash
cargo build -p tddy-remote
```

### Testing

```bash
cargo test -p tddy-remote
```

## Architecture

The binary loads YAML authority definitions, resolves Connect base URLs, and uses **`tddy-connectrpc`**-compatible HTTP POSTs for unary RPCs; the **RSH** path multiplexes stdio with a TCP session opened via **`OpenRsyncSession`**.

## Documentation

### Product requirements (what)

- [Remote sandbox feature](../../docs/ft/daemon/remote-sandbox.md)

### Technical implementation (how)

- [Remote sandbox client](./docs/remote-sandbox-client.md)
- [Changesets](./docs/changesets.md)

## Related packages

- [tddy-service](../tddy-service/README.md) — **`RemoteSandboxService`** implementation and protos
- [tddy-daemon](../tddy-daemon/README.md) — daemon HTTP + RPC registration
