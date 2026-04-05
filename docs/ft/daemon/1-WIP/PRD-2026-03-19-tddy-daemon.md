# PRD: tddy-daemon — Multi-User Daemon Binary

**Status:** 🚧 In progress — Phase 1 shipped; extended success criteria below remain the product target
**Created:** 2026-03-19

## Summary

Extract a new `tddy-daemon` binary from the existing `tddy-coder --daemon` mode. The daemon is a root-level system process (systemd-managed) that orchestrates multi-user access to tddy-* tools. It handles GitHub authentication, maps GitHub users to OS users, spawns tddy-* processes as the target OS user (fork + setuid/setgid), and manages session discovery. Each spawned process joins its own LiveKit room with daemon-provided credentials.

`tddy-coder --daemon` is retained as-is for single-user dev/personal use.

## Implementation status (2026-03-21)

**Delivered in tree:** `packages/tddy-daemon` binary; HTTP static bundle; GitHub OAuth via Connect-RPC (`AuthService`); user mapping and `allowed_tools`; Unix spawn with `pre_exec` + `setuid`/`setgid` when spawning as a different OS user (`spawner.rs`); LiveKit credentials passed to spawned `tddy-coder --daemon`; session metadata including `--project-id` / `.session.yaml`; project-centric connection UX in tddy-web (projects, sessions, `StartSession` with `project_id`) — see [daemon project concept](../project-concept.md) and [Web changelog](../../web/changelog.md).

**Superseded UX:** The connection screen no longer uses a single global “repository path” field; work is scoped per project as documented in [web-terminal.md](../../web/web-terminal.md).

**Still open vs this PRD:** Validate each numbered success criterion against production behavior (especially resume, session table columns, and central auth storage guarantees). Update this document when the checklist is fully satisfied.

## Background

Currently `tddy-coder --daemon` serves all roles: web server, GitHub OAuth, LiveKit coordination, session management, gRPC server, and TUI streaming. This works for single-user development but does not support:

- Multi-user access with authentication and authorization
- Running as a system service that spawns processes as different OS users
- Centralized auth token management
- Per-user session isolation with OS-level security boundaries

The daemon needs to be a separate binary with its own configuration, logging, and process lifecycle — suitable for running under systemd as a root process.

## Affected Features

- [coder/grpc-remote-control.md](../../ft/coder/grpc-remote-control.md) — Daemon mode section describes current `--daemon` behavior; `tddy-daemon` takes over the multi-user orchestration role
- [web/web-terminal.md](../../ft/web/web-terminal.md) — Connection flow changes; authentication and session selection happen before terminal streaming
- [coder/1-OVERVIEW.md](../../ft/coder/1-OVERVIEW.md) — tddy-coder retains `--daemon` for single-user; daemon-specific features move to tddy-daemon

## Requirements

### 1. Separate Binary: `tddy-daemon`

New Cargo binary crate at `packages/tddy-daemon/`. Runs as a root system process. Does **not** run any TDD workflow itself — its sole purpose is orchestration.

Responsibilities:
- Serve tddy-web static bundle over HTTP
- Handle GitHub OAuth flow (AuthService RPC via Connect-RPC)
- Map authenticated GitHub users to OS users via config
- Present connection screen data to tddy-web (available tools, user sessions)
- Spawn tddy-* processes as the mapped OS user
- Manage LiveKit room allocation (one room per session)

### 2. GitHub Authentication Migration

All GitHub authentication moves from tddy-coder to tddy-daemon:
- `tddy-github` crate remains a shared library
- `AuthService` RPC runs exclusively in tddy-daemon
- Auth tokens stored centrally (path configurable via config or ENV, e.g., `/var/lib/tddy-daemon/auth/`)
- GitHub OAuth flags (`--github-client-id`, `--github-client-secret`, `--github-redirect-uri`) move to daemon config
- `tddy-coder` no longer needs GitHub auth flags when spawned by daemon

### 3. User Mapping & Authorization

Daemon config maps GitHub users to OS users:

```yaml
users:
  - github_user: "octocat"
    os_user: "dev1"
  - github_user: "torvalds"
    os_user: "dev2"
```

- If a GitHub user has no mapping → access denied (clear error shown in tddy-web)
- Mapping is checked after successful GitHub OAuth

### 4. Tool Allowlist

Config specifies which tddy-* binaries are available:

```yaml
allowed_tools:
  - path: "target/debug/tddy-coder"
    label: "tddy-coder (debug)"
  - path: "target/release/tddy-coder"
    label: "tddy-coder (release)"
  - path: "/usr/local/bin/tddy-coder"
    label: "tddy-coder (system)"
```

Each entry becomes an option in the connection screen dropdown.

### 5. Connection Screen (tddy-web)

After authentication, the user sees a connection screen with:

1. **Tool dropdown** — lists allowed tddy-* tools from daemon config
2. **Repository path** — text input for the repo to work in
3. **Session list** — from the mapped OS user's `~/.tddy/sessions/`:
   - **Columns**: session ID, date, current status, repo path, PID of owning process
   - **Active sessions** (owning PID is alive) → "Connect" button
   - **Inactive sessions** (no live PID) → "Resume" button

### 6. Session Metadata

Each session directory gains a `.session.yaml` file with:

```yaml
session_id: "uuid"
created_at: "2026-03-19T10:00:00Z"
updated_at: "2026-03-19T10:30:00Z"
status: "active"  # active, completed, failed, idle
repo_path: "/home/dev1/projects/myapp"
pid: 12345
tool: "target/release/tddy-coder"
livekit_room: "session-uuid"
```

- Written by the spawned tddy-* process (tddy-coder writes it on startup)
- Read by tddy-daemon for session listing
- PID used to determine if session is active (check if process is alive)

### 7. Process Spawning

Daemon spawns tddy-* processes as the target OS user:

- **Mechanism**: fork + setuid/setgid to target user
- **Flags passed**: `--daemon --livekit-url <url> --livekit-api-key <key> --livekit-api-secret <secret> --livekit-room <session-room>`
- **CWD**: set to the repository path
- Each process runs its own VirtualTui and joins LiveKit directly
- Daemon does **not** capture or proxy STDIO

### 8. Resume Flow

When resuming a session:
- Daemon spawns a new tddy-* process with `--resume-from <session-id>` flag
- CWD set to the session's repo path (from `.session.yaml`)
- The new process picks up session state from `~/.tddy/sessions/<id>/`

### 9. Daemon Configuration

YAML format, consistent with existing tddy-coder config:

```yaml
listen:
  web_port: 8899
  web_host: "0.0.0.0"

web_bundle_path: "/opt/tddy/web"

livekit:
  url: "ws://localhost:7880"
  api_key: "devkey"
  api_secret: "secret"

github:
  client_id: "..."
  client_secret: "..."
  redirect_uri: "http://localhost:8899/api/auth/callback"

auth_storage: "/var/lib/tddy-daemon/auth"
log_dir: "/var/log/tddy-daemon"

users:
  - github_user: "octocat"
    os_user: "dev1"
  - github_user: "torvalds"
    os_user: "dev2"

allowed_tools:
  - path: "target/debug/tddy-coder"
    label: "tddy-coder (debug)"
  - path: "target/release/tddy-coder"
    label: "tddy-coder (release)"
```

### 10. Retain `tddy-coder --daemon`

`tddy-coder --daemon` remains exactly as-is:
- Single-user daemon for dev/personal use
- Retains GitHub auth flags (optional, for direct use without tddy-daemon)
- Retains web server, LiveKit, gRPC
- No user mapping or multi-user features

## Success Criteria

1. `tddy-daemon` binary starts, serves web bundle, handles GitHub OAuth
2. GitHub user maps to OS user; unmapped users see access denied
3. Connection screen shows tool dropdown, repo path input, session list with correct status
4. Daemon spawns tddy-coder as target OS user with LiveKit credentials
5. Spawned process joins LiveKit room; browser connects and streams terminal
6. Resume flow spawns new process with `--resume-from <id>` and correct CWD
7. `tddy-coder --daemon` continues working as before (no regression)
8. Session `.session.yaml` is written and read correctly
9. Process spawning uses fork + setuid/setgid (not su/sudo)
10. Central auth storage works across daemon restarts

## Scope Boundaries

**In scope:**
- New `tddy-daemon` binary
- GitHub auth migration to daemon
- User mapping and authorization
- Connection screen in tddy-web
- Session metadata (`.session.yaml`)
- Process spawning as OS user
- Resume from session
- Daemon YAML config

**Out of scope (future):**
- Role-based access control beyond user mapping
- Per-user tool allowlists (currently global)
- Audit logging
- Session transfer between users
- Container-based process isolation
