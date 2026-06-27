# PRD: Darwin-Sandboxed Claude Code CLI Sessions (Local gRPC)

**Status:** In-Progress · **Date:** 2026-06-27 · **Product area:** daemon

## Summary

Run the Claude Code CLI inside a macOS Seatbelt sandbox (`sandbox-exec` + SBPL
profile) on the same host as `tddy-daemon`. The daemon spawns the sandbox, dials
into a gRPC server exposed by the sandbox, and serves the host codebase to the
sandboxed agent over that channel — no LiveKit, no remote checkout. A
cross-platform `Sandbox` abstraction is introduced so Linux can provide an
implementation later (unsupported stub for now).

This is the local, sandboxed counterpart of **Remote-Codebase Mode**: the agent
runs "remote" (inside the sandbox jail) and accesses the codebase exclusively
via `mcp__tddy-tools__*` tool calls that flow back to the daemon over a
daemon-established gRPC channel; the daemon executes them against the host
codebase. The transport is direct local gRPC instead of the LiveKit relay.

## Background and motivation

- Today `claude-cli` sessions spawn the `claude` binary in a PTY + git worktree
  with **no filesystem confinement** — a runaway agent can write anywhere the
  OS user can.
- `expo-darwin-sandbox` (reference repo at `~/Code/expo-darwin-sandbox`) proves
  that macOS Seatbelt (`sandbox-exec` + SBPL) can confine a build process to
  writes inside a project + scratch + egress while still reading toolchains.
  Claude still needs LLM API egress; **outbound network from the jail is denied**.
  The only egress path is the **host-established inbound gRPC `SessionChannel`**
  — the host daemon relays HTTP requests to the internet and returns responses
  on the same bidi stream. (Updated: 2026-06-27)
- Remote-Codebase Mode already established the agent-side model: native
  filesystem tools are replaced by `mcp__tddy-tools__*` MCP tools, the allowlist
  excludes `Read`/`Write`/`Edit`/`Glob`/`Grep`/`Bash(...)`, and `tool_engine`
  executes tools against a worktree. We reuse that model, only the transport
  changes (local gRPC bidi channel instead of LiveKit relay).

## Affected features

- `docs/ft/daemon/claude-cli-session.md` — the claude-cli session gains a
  sandboxed variant; spawn path wraps the claude process in a darwin sandbox.
- `docs/ft/daemon/remote-codebase-mode.md` — this is the local-sandbox sibling
  of remote-codebase mode; reuses `mcp__tddy-tools__*` allowlist,
  `RemoteContextDir`, and `tool_engine::ExecuteTool`. The transport is local
  gRPC, not LiveKit.
- `docs/ft/daemon/changelog.md` — new entry on merge.

## High-level requirements

### Sandbox mechanism (darwin)

1. **Seatbelt `sandbox-exec` + SBPL profile**, generated from a template with
   path placeholders. **Tight policy**: in addition to write-confinement
   (project + scratch + egress + device nodes), reads are restricted to an
   explicit allow-list (toolchain subpaths — Xcode, Node, Homebrew — system
   libs, and the sandbox's own project tree). Not the broad `(allow default)`
   reads of the reference PoC.
2. The sandbox process is launched with a clean, explicit environment
   (`env -i`) and a redirected `HOME`/`TMPDIR` into a per-sandbox `.work/` so
   caches land inside the jail, not the real `$HOME`.
3. A negative confinement test (write outside the jail must be denied) is part
   of acceptance.
4. **Network egress from the jail is denied** (`(deny network*)` in SBPL). The
   sandbox cannot open outbound TCP/UDP sockets. All external reachability
   (LLM APIs, MCP tool side-effects on the host) goes through the host-dialled
   `SessionChannel`. (Updated: 2026-06-27)

### Cross-platform abstraction

5. A new shared **`tddy-sandbox`** crate defines a `Sandbox` trait (prepare
   profile, spawn a command inside the sandbox, tear down) and SBPL-independent
   types (`SandboxSpec`, `SandboxHandle`). No darwin-only symbols in the trait.
6. A new **`tddy-sandbox-darwin`** crate provides the Seatbelt impl
   (`DarwinSeatbeltSandbox`): renders the SBPL profile from a template, invokes
   `sandbox-exec -f <profile>`, returns a handle to the jailed process.
7. On non-darwin targets the `tddy-sandbox` facade returns
   `Unsupported { platform: "...", message: "darwin Seatbelt sandboxes are not
   available on this OS" }` — no panic, no fallback behavior, a clear error.

### RPC surface (daemon → client) — service level only, no UI

8. A **`sandbox: bool`** flag on `StartSessionRequest` (or a new
   `session_type: "sandboxed-claude-cli"` — to be settled in Plan mode) selects
   the sandboxed spawn path. Service-level only; **no `tddy-web` UI changes in
   this PR**.
9. The daemon exposes local (loopback) gRPC only. **No LiveKit** for sandboxed
   sessions — `ConnectSession`/`ResumeSession` return empty LiveKit credentials,
   mirroring `claude-cli`/`workspace` sessions.
10. **No auth on the local sandbox gRPC path** (loopback only), consistent with
    the user's selection. Existing session-token auth on the daemon's public
    `ConnectionService` is unchanged.

### gRPC direction & codebase access (daemon ↔ sandbox)

11. **The sandbox exposes a gRPC server**; **the daemon dials into it**. The
    sandbox never initiates a TCP connection back to the daemon. All sandbox→host
    data (terminal, tool exec, **HTTP egress**) is sent as replies on the
    **inbound-established** bidi `SessionChannel`.
12. **Single host-initiated bidi `SessionChannel`** (replaces earlier split-RPC
    design with `StreamSandboxTerminalOutput` + `SandboxToolExecChannel`).
    Tonic does not pump the server outbound stream until the client sends inbound
    frames, so **all sandbox→host traffic is host-poll driven**:
    - Host sends: `SubscribeTerminal`, `HostPoll`, `SandboxInput`,
      `ExecuteToolResponse`, `EgressResponse` (Updated: 2026-06-27)
    - Sandbox replies (only after inbound): `SessionTerminalOutput`,
      `ExecuteToolRequest`, `EgressRequest` (Updated: 2026-06-27)
    - Terminal I/O, MCP tool-exec, and **LLM HTTP egress** share this one bidi
      stream; there is no unprompted sandbox outbound on gRPC.
13. MCP tool path inside the jail: `tddy-tools --mcp` → unix domain IPC →
    `SandboxSessionRelay` queue → flushed as `ExecuteToolRequest` on the next
    `HostPoll` → host runs `tool_engine::ExecuteTool` → `ExecuteToolResponse`
    back on the client stream.

### LLM egress via SessionChannel (Updated: 2026-06-27)

**Supersedes:** an interim design that used a host loopback TCP/HTTP proxy with
`HTTPS_PROXY` inside the jail. Outbound network is denied; the proxy path is
not viable under `(deny network*)`.

14. Claude Code requires HTTP(S) access to LLM provider APIs. Because outbound
    sockets from the jail are **denied**, egress uses the **same `SessionChannel`**
    as terminal I/O and MCP tools:
    - An **in-jail HTTP shim** (inside `sandbox-runner`, or a small local
      forwarder bound to loopback inside the jail only) accepts Claude's HTTP
      client traffic without leaving the process tree.
    - The shim enqueues **`EgressRequest`** frames (method, URL, headers, body)
      on the `SandboxSessionRelay`, flushed on the next **`HostPoll`** — same
      pattern as MCP `ExecuteToolRequest`.
    - The **host daemon** performs the outbound HTTP(S) call to the internet and
      returns **`EgressResponse`** (status, headers, body) on the client stream.
15. Seatbelt policy: **`(deny network*)`** — no outbound TCP/UDP from the jail.
    Acceptance: direct socket probes from inside the jail fail; LLM reachability
    succeeds only when the host relays via `SessionChannel`.
16. Fake claude for tests: reuse **`tddy-demo-tui`** (same binary as Cypress e2e)
    to assert PTY dimension output (`DEMO TUI W=`) without a real Claude install.
    Egress acceptance uses a shim-aware probe script, not `HTTPS_PROXY` / `curl -x`.

### Agent model inside the sandbox

17. The sandbox entrypoint (`tddy-tools sandbox-runner`) starts the in-jail gRPC
    server, then execs `claude` in a PTY. The claude process uses
    **`mcp__tddy-tools__*` tools only** — native filesystem/shell tools are
    excluded via `--allowedTools` (reusing `remote_codebase_allowlist` +
    `build_remote_allowlist`). _Not yet wired in spawn argv._
18. The codebase is **NOT mounted into the sandbox filesystem**. The agent
    accesses it exclusively via `mcp__tddy-tools__*` tool calls over
    `SessionChannel`. This keeps the tight read-confinement policy simple.
19. A read-only **context dir** (reusing `RemoteContextDir`) containing synced
    `CLAUDE.md`/`AGENTS.md`/skills + the `REMOTE_APPENDIX` notice is placed
    inside the sandbox; the agent's working directory is this read-only dir.

### Host placement

20. The sandbox runs on the **same host as the daemon** the client addresses.
    "Which host" = which daemon instance you talk to (multi-daemon, each local
    to its host). No SSH-launch, no host scheduler in this PR.

## Architecture (confirmed — SessionChannel for all host relay) (Updated: 2026-06-27)

```
 Client (tests / future UI / Cypress e2e with tddy-demo-tui)
   │  local gRPC (loopback, no auth)
   ▼
 tddy-daemon (host, owns the codebase + internet egress)
   │  ConnectionService: StartSession(sandbox=true) / StreamTerminalOutput / SendTerminalInput
   │  - spawns sandbox via tddy-sandbox-darwin (sandbox-exec -f <profile>, deny network*)
   │  - waits for sandbox gRPC server, dials INTO it (only egress path from jail)
   │  - single SessionChannel bidi loop (dial_and_bridge):
   │       host → SubscribeTerminal, HostPoll, SandboxInput, ExecuteToolResponse, EgressResponse
   │       sandbox → TerminalOutput, ExecuteToolRequest, EgressRequest (only after host inbound)
   │  - host performs outbound HTTP(S) to LLM APIs when EgressRequest arrives
   ▼
 sandbox (darwin Seatbelt jail — no outbound network)
   ├─ tddy-tools sandbox-runner (in-jail gRPC server)
   │    └─ SessionChannel (terminal + tools + HTTP egress on one bidi RPC)
   ├─ MCP: tddy-tools --mcp  →  unix IPC  →  relay queue  →  HostPoll flush
   ├─ HTTP shim: Claude HTTP  →  EgressRequest  →  HostPoll flush  →  host internet
   ├─ claude CLI  (--allowedTools = mcp__tddy-tools__* + AskUserQuestion)  [pending wiring]
   └─ read-only context dir (CLAUDE.md/AGENTS.md/skills + REMOTE_APPENDIX)
```

**Design notes:**
- An earlier draft split terminal streaming and tool-exec into separate sandbox
  RPCs (`StreamSandboxTerminalOutput`, `SandboxToolExecChannel`). Abandoned
  because tonic's bidi server stream does not emit until the client sends the
  first inbound frame.
- An interim draft used a host loopback **TCP proxy + `HTTPS_PROXY`**. Abandoned
  because outbound network from the jail is denied; the only way out is replies
  on the host-established `SessionChannel`.

## Success criteria (high-level — detailed AC in the changeset)

- A `StartSession` with the sandbox flag spawns `claude` inside a darwin
  Seatbelt sandbox on the daemon's host; the daemon dials the sandbox's gRPC
  server; terminal I/O flows client → daemon → `SessionChannel` → claude PTY and back.
- The sandboxed `claude` reads/writes the host codebase only through
  `mcp__tddy-tools__*` tool calls; native file/shell tool calls are denied.
- **Outbound network from the jail is denied.** In-jail Claude reaches LLM APIs
  only when the host relays **`EgressRequest`** frames on `SessionChannel` and
  returns **`EgressResponse`**. (Updated: 2026-06-27)
- A write outside the jail is denied by the kernel (confinement enforced).
- On a non-darwin target, requesting a sandboxed session returns a clear
  "unsupported on this OS" error — no fallback, no panic.
- No LiveKit is involved; the daemon's public auth surface is unchanged.
- Acceptance tests can assert sandbox PTY behavior using **`tddy-demo-tui`**
  (same fake claude binary as Cypress e2e).

## Non-goals (out of scope for this PR)

- `tddy-web` UI for sandboxed sessions (service-level only).
- Launching sandboxes on other hosts over SSH; host scheduler / pool.
- Linux sandbox implementation (only the abstraction + unsupported stub).
- Cross-platform sandbox profile formats beyond darwin SBPL.
- Changing the daemon's public `ConnectionService` auth model.
- Read-confinement of the host codebase path (the codebase is not in the
  sandbox fs at all — accessed via tool calls only).

## References

- Reference repo: `~/Code/expo-darwin-sandbox` (`build.sh`, `scripts/inner-build.sh`,
  `profiles/expo-ios.sb.tmpl`).
- `packages/tddy-service/proto/sandbox.proto` — **`SessionChannel`** (implemented RPC surface).
- `packages/tddy-tools/src/sandbox_runner.rs` — in-jail server + `SandboxSessionRelay`.
- `packages/tddy-daemon/src/sandbox_session.rs` — `dial_and_bridge`, `wait_for_sandbox_ready`.
- `packages/tddy-demo-tui` — fake claude CLI for Cypress e2e and sandbox acceptance tests.
- `packages/tddy-coder/src/remote.rs` — `RemoteContextDir`, `REMOTE_APPENDIX`,
  `build_remote_allowlist`.
- `packages/tddy-daemon/src/tool_engine.rs` — `ExecuteTool` dispatch.
- `packages/tddy-daemon/src/connection_service.rs` — `start_sandboxed_claude_cli_session`,
  `stream_terminal_output` / `send_terminal_input`; `session_type` dispatch.
- `packages/tddy-daemon/src/claude_cli_session.rs` — `build_claude_argv`,
  `spawn_in_pty`.
- `packages/tddy-workflow-recipes/src/permissions.rs` —
  `remote_codebase_allowlist`.
- `docs/ft/daemon/remote-codebase-mode.md` — sibling feature.
- Changeset (living doc): `docs/dev/1-WIP/2026-06-27-darwin-sandbox-claude-cli.md`.
