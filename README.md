# tddy-coder

TDD-focused development workflow with a web dashboard, daemon, and Claude Code CLI integration.

## Packages

| Package | Type | Description |
|---------|------|-------------|
| `tddy-core` | Library | `CodingBackend` trait, workflow state machine, output parser, Claude/Mock backends |
| `tddy-coder` | Binary | CLI: `--goal plan` reads stdin, produces `PRD.md` + `TODO.md` |
| `tddy-daemon` | Binary | HTTP/gRPC server managing sessions, projects, PTY relay |
| `tddy-tools` | Binary | CLI utilities: `pty-relay`, install helpers |
| `tddy-web` | Web app | React dashboard (Vite + Storybook + Cypress) |

---

## Setup

Requires [Nix](https://nixos.org/). One-time setup:

```bash
nix flake lock      # generate flake.lock
nix develop         # enter dev shell
```

With **direnv**: `direnv allow` once — shell loads automatically on `cd`.

---

## Running Options

### 1. Local development (recommended)

Starts `tddy-daemon` and the Vite dev server with `/rpc` proxied to the daemon.

```bash
./web-dev
```

- Web UI: http://localhost:5173
- Daemon: http://127.0.0.1:8899 (configured in `dev.daemon.yaml`)
- Config file: `dev.daemon.yaml`

### 2. Daemon only (headless)

Run just the daemon without the Vite dev server — useful for production or when serving a pre-built bundle:

```bash
cargo run -p tddy-daemon -- --config dev.daemon.yaml
# or after ./release:
./target/release/tddy-daemon --config dev.daemon.yaml
```

The daemon serves the pre-built web bundle from `packages/tddy-web/dist` (set `web_bundle_path` in config). Build the bundle first with `bun run build`.

### 3. Production install (systemd)

```bash
./release                          # build optimized binaries
bun run build                      # build web bundle
sudo ./install --systemd           # install daemon + web bundle + systemd unit
```

Environment overrides:

| Variable | Default | Purpose |
|----------|---------|---------|
| `INSTALL_PREFIX` | `/usr/local` | Root install prefix |
| `INSTALL_BIN_DIR` | `$PREFIX/bin` | Binary destination |
| `INSTALL_CONFIG_DIR` | `/etc/tddy` | Config destination |
| `INSTALL_SYSTEMD_DIR` | `/etc/systemd/system` | Unit file destination |
| `INSTALL_WEB_BUNDLE_DIR` | `/var/tddy/web` | Web bundle destination |
| `INSTALL_NO_SYSTEMCTL=1` | — | Skip root check and `systemctl` (test harness) |

### 4. Remote daemon — connect a local PTY proxy

`tddy-tools pty-relay` can attach a local terminal to a Claude CLI session running on a remote daemon. Three modes:

#### 4a. Start a new session and connect (LiveKit)

Creates a session on the remote daemon, then connects your terminal via LiveKit bidirectional stream.

```bash
tddy-tools pty-relay \
  --daemon-url http://<host>:<port> \
  --daemon-identity <instance-id> \
  --project-id <project-id> \
  --livekit-url ws://<host>:7880 \
  --livekit-room tddy-lobby
```

Flow: `StartSession` RPC → worktree creation → LiveKit participant appears → `terminal.TerminalService/StreamTerminalIO` bidi stream → stdin/stdout relay.

#### 4b. Connect to an existing session (LiveKit)

Attach to a session that was already started (e.g. via the web UI). Get `livekit_server_identity` from the session list.

```bash
tddy-tools pty-relay \
  --livekit-url ws://<host>:7880 \
  --livekit-room tddy-lobby \
  --server-identity daemon-<instance_id>-<session_id>
```

Multiple terminals can attach simultaneously — the daemon broadcasts PTY output and replays a 64 KB capture buffer to late subscribers.

#### 4c. Connect to an existing session (gRPC, no LiveKit)

Browser-compatible path using server-streaming output + unary input. Use when LiveKit is not configured on the remote daemon.

```bash
tddy-tools pty-relay \
  --daemon-url http://<host>:<port> \
  --session-id <session-id> \
  --session-token <token>
```

Uses `StreamTerminalOutput` (server-streaming) + `SendTerminalInput` (unary) — the same RPCs the web UI uses.

#### 4d. Local PTY (no daemon)

Spawn a command in a local PTY for testing the relay itself without any network connection.

```bash
tddy-tools pty-relay -- claude --model claude-opus-4-8
```

### 5. Storybook (component development)

```bash
./dev bun run storybook     # dev server at http://localhost:6006
```

---

## Configuration (`dev.daemon.yaml`)

Key options:

```yaml
listen:
  web_port: 8899
  web_host: 127.0.0.1

daemon:
  daemon_instance_id: dev

web:
  web_bundle_path: packages/tddy-web/dist

# Optional: LiveKit for PTY bridging over WebRTC
livekit:
  url: ws://127.0.0.1:7880
  api_key: devkey
  api_secret: secret
  common_room: tddy-lobby

# Optional: Claude Code CLI session support
claude_cli:
  binary_path: claude        # resolved from PATH, or absolute path
```

Without the `livekit` block, Claude CLI sessions fall back to the gRPC `StreamTerminalOutput`/`SendTerminalInput` path automatically.

---

## Claude CLI Sessions

From the web UI, select session type **Claude CLI** and a model to start an interactive `claude` process in its own git worktree:

- Worktree: `<repo>/.worktrees/claude-cli-<id>/` on branch `claude-cli/<id>`
- Session metadata: `~/.tddy/sessions/<session_id>/.session.yaml`
- Resume: click **Resume** on an inactive session — spawns a new `claude` process in the existing worktree
- Delete: terminates the process and removes the worktree

Terminal resize is relayed via the escape sequence `\x1b]resize;<cols>;<rows>\x07`.

---

## Build & Test Reference

| Action | Command |
|--------|---------|
| Build (debug) | `cargo build` |
| Build (release) | `./release` |
| Test all | `./test` |
| Test one package | `./test -p tddy-core` |
| Test one case | `./test -- test_name` |
| Lint | `cargo clippy -- -D warnings` |
| Format | `cargo fmt` |
| Web install | `./dev bun install` |
| Web build | `./dev bun run build` |
| Cypress component | `./dev bun run cypress:component` |
| Cypress e2e | `./dev bun run cypress:e2e` |

If the agent terminal doesn't capture test output, run `./verify` — it writes results to `.verify-result.txt`.

---

## LiveKit Testkit

Run LiveKit integration tests against a persistent container instead of spawning one per run:

```bash
./run-livekit-testkit-server   # prints LIVEKIT_TESTKIT_WS_URL=ws://127.0.0.1:PORT

export LIVEKIT_TESTKIT_WS_URL=ws://127.0.0.1:PORT
cargo test -p tddy-livekit -p tddy-livekit-testkit
```

Or combined: `eval $(./run-livekit-testkit-server | grep '^export ')`.
