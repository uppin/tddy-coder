# Remote Managed Worktree — choose which daemon holds the codebase

> Extends [managed-codebase mode](remote-codebase-mode.md). That feature made the agent reach its
> codebase only through `mcp__tddy-tools__*` exec tools; this one lets the codebase live on a
> **different daemon** than the agent, chosen per session from the web UI.

## Summary

Today a session has exactly one host axis: `StartSessionRequest.daemon_instance_id` decides where
the agent *and* its git worktree go, together. Managed-codebase mode already removes the agent's
native filesystem access, but the worktree it proxies to is still created on the same daemon that
runs the agent.

This feature splits that axis. A session gains a second, independent placement:

- **`daemon_instance_id`** — where the agent process runs (host A). Unchanged.
- **`codebase_daemon_instance_id`** — where the git worktree lives (host B). New.

When the two differ, host A's daemon creates a `workspace` session on host B (over the existing
LiveKit peer-forward path), spawns the agent locally with **no repo on disk**, and the agent's
`tddy-tools --mcp` reaches the worktree by calling `ConnectionService/ExecuteTool` **directly on
daemon B over LiveKit RPC**.

This is the same tool-proxy model the sandbox already uses over stdio — `tddy-tools` speaks
`tddy-rpc`, and LiveKit is simply another binding for it, alongside stdio and HTTP/Connect.

```
host A                                    host B
┌────────────────────────────┐            ┌──────────────────────────┐
│ claude (claude-cli)        │            │ tddy-daemon (B)          │
│   no Read/Write/Bash       │            │   workspace session      │
│   ↓ MCP (stdio)            │            │   git worktree  ← real   │
│ tddy-tools --mcp           │            │   tool_engine::execute   │
│   ↓ tddy-rpc               │            └──────────▲───────────────┘
│   LiveKit RpcClient        │  LiveKit data channel │
│     → daemon-B ────────────┼───────────────────────┘
└────────────────────────────┘
  tddy-daemon (A): spawns the agent, mints the room token,
                   owns the paired B-side session's lifecycle
```

## User Story

As an operator, I want to start an agent session on my laptop while the repository it works on
stays on my workstation — because the workstation has the checkout, the build cache, and the
toolchain — so that I can drive a coding agent against a codebase I have not cloned locally, and
choose that placement from the new-session form rather than a CLI flag.

## Placement model

| `daemon_instance_id` | `codebase_daemon_instance_id` | Result |
|---|---|---|
| `A` | empty | Co-located. Exactly today's behaviour. |
| `A` | `A` | Co-located. Explicit form of the above. |
| `A` | `B` | **Split.** Agent on A, worktree on B. |

`codebase_daemon_instance_id` is only meaningful together with `managed_codebase = true`: an agent
that still has native filesystem tools has nothing to proxy through. A split placement without
`managed_codebase` is a request error, not a silently co-located session.

### What a split session cannot also ask for

Three otherwise-valid options need a repository on the daemon running the agent, which a split
session does not have. Each is **refused** with `invalid_argument` naming the field, rather than
silently dropped — a session that came up without its recipe looks exactly like the session that
was asked for.

| Field | Why it needs a local repository |
|---|---|
| `recipe` | A workflow recipe's tooling resolves `TDDY_REPO_DIR` on the agent's host |
| `semantic_index` | Indexes a worktree on this daemon before launch |
| `sandbox` | The sandboxed spawn resolves its worktree on this daemon |

This mirrors the v1 restriction the original remote-codebase mode already carries (recipes other
than `free-prompting` were out of scope there too).

**The UI must not offer them.** `CreateSessionPane` defaults `recipe` to `"tdd"` and sends it
whenever managed codebase is on — so without a matching gate, the *only* thing the codebase-host
selector could produce is a request the daemon rejects. The form therefore withdraws the Recipe
control once a codebase host is chosen, and sends an empty `recipe`. Putting the codebase back on
the session's own host restores it: the withdrawal is a property of the split, not a one-way door.

### Why claude-cli only

v1 restricts split placement to `session_type: "claude-cli"`. A `cursor-cli` request carrying
`codebase_daemon_instance_id` is refused with a clear error rather than silently co-located.

The reason is enforceability. For claude-cli, managed-codebase is enforced by construction:
`build_claude_allowlist` maps the discovered exec tools to `mcp__tddy-tools__*` and emits
`--allowedTools`, plus `--disallowedTools` covering the native aliases. A native `Read` or `Bash`
call is impossible, not merely discouraged.

`cursor-agent` has no `--allowedTools`/`--disallowedTools` equivalent anywhere in this codebase, so
a split cursor session could only be *guided* — via `REMOTE_APPENDIX` and a `.cursor/rules/*.mdc`
entry — never *prevented* from attempting native filesystem access. It would also need a context
dir, an MCP config, tool-relay env, and resume-env plumbing that the non-sandboxed cursor path
does not have today (`cursor_cli_spawn.rs:302` discards `managed_codebase` outright). That is
substantial work for a materially weaker guarantee, so it is deferred and tracked in
`docs/dev/TODO.md`.

## API surface

### `StartSessionRequest` (connection.proto)

```proto
// Daemon instance whose filesystem holds this session's git worktree. Empty or matching
// `daemon_instance_id` = co-located (the pre-existing behaviour). When it names a different
// eligible daemon, that daemon creates a `workspace` session holding the worktree and the agent
// reaches it only via mcp__tddy-tools__* over LiveKit.
// Requires managed_codebase = true and session_type = "claude-cli". A cursor-cli request
// carrying this field is refused (see § Why claude-cli only).
string codebase_daemon_instance_id = 32;
```

### `SessionEntry` (connection.proto)

```proto
// Daemon instance holding this session's worktree, when it is not the daemon running the agent.
// Empty for co-located sessions. Lets the web render "agent on A / codebase on B".
string codebase_daemon_instance_id = 29;
// The paired `workspace` session on that daemon whose worktree this session works in.
// Empty for co-located sessions.
string codebase_session_id = 30;
```

### `SessionMetadata` (`.session.yaml`, tddy-core)

```rust
/// Daemon instance holding this session's worktree. Absent for co-located sessions and legacy
/// files. Persisted, unlike `SessionEntry.daemon_instance_id`, which is stamped at read time —
/// a split session cannot be attributed by "who answered ListSessions", because two daemons
/// each legitimately hold one half.
pub codebase_daemon_instance_id: Option<String>,
/// The paired `workspace` session id on that daemon. Absent for co-located sessions.
pub codebase_session_id: Option<String>,
```

For a split session, host A's `.session.yaml` has **`repo_path: None`** — there is no repository on
A. Every consumer that reads `repo_path` to reach a worktree must treat a split session as
"worktree is elsewhere", not as a malformed session.

### `RemoteToolEnv` (tddy-core)

Gains the credential needed for the LiveKit binding. The existing `livekit_url`, `livekit_room`
and `server_identity` fields are populated for the first time.

```rust
/// Scoped LiveKit join token minted by the spawning daemon. Exported as
/// TDDY_REMOTE_LIVEKIT_TOKEN. Grants room-join for exactly this session's room under a pinned
/// participant identity — never the daemon's `livekit.api_secret`, which would let the agent
/// join any room as any identity.
pub livekit_token: Option<String>,
```

No new minting machinery is needed: `tddy_livekit::TokenGenerator::new(api_key, api_secret, room,
identity, ttl).generate()` already produces exactly this shape, with grants `room_join`,
`can_publish`, `can_subscribe`, `can_publish_data` and `can_update_own_metadata` — `can_publish_data`
being the one the RPC data channel requires. The daemon calls it at spawn with the common room and
a session-scoped participant identity.

This deliberately does **not** follow the precedent at `spawner.rs:886-902`, which passes
`--livekit-api-secret` to spawned `tddy-coder` on the command line, where it is readable from
`/proc/<pid>/cmdline`. An agent process running model-authored code is not the place for a
credential that mints tokens for any room.

Env contract exported to the agent process:

| Variable | Meaning |
|---|---|
| `TDDY_REMOTE_LIVEKIT_URL` | LiveKit server URL |
| `TDDY_REMOTE_LIVEKIT_ROOM` | Common room name |
| `TDDY_REMOTE_LIVEKIT_TOKEN` | Scoped join JWT (new) |
| `TDDY_REMOTE_SERVER_IDENTITY` | `daemon-{B}` — the RPC-server participant to address |
| `TDDY_REMOTE_SESSION_ID` | The **B-side workspace session id** |
| `TDDY_REMOTE_SESSION_TOKEN` | Caller's session token, verified on B |
| `TDDY_REMOTE_DAEMON_INSTANCE_ID` | `B` |

### `SessionToolTransport` (tddy-tools)

A third variant beside the existing two. `tddy_livekit::RpcClient` already implements
`tddy_rpc::RpcClientTransport`, so the dispatch function is reused unchanged apart from carrying
the request envelope (see below).

```rust
pub enum SessionToolTransport {
    SandboxIpc { socket_path: PathBuf },
    DaemonHttp { /* … */ },
    /// Direct LiveKit RPC to a remote daemon's ConnectionService.
    LiveKit {
        url: String,
        room: String,
        token: String,
        server_identity: String,
        session_id: String,
        session_token: String,
        daemon_instance_id: String,
    },
}
```

Detection order in `detect_session_tool_transport()` is `SandboxIpc` → `LiveKit` → `DaemonHttp`, so
an in-jail session keeps its stdio path even when LiveKit env is also present.

### `StreamExecuteTool` (connection.proto)

The unary `ExecuteTool` returns `result_json` as one string. Over LiveKit any response above
`MAX_CHUNK_FRAME_BYTES` (60 000) is chunk-framed, and chunk reassembly is best-effort and
index-keyed — a lost frame wedges the call permanently with no error. A `Read` of a large file or a
broad `Grep` crosses that on day one.

A server-streaming sibling carries the result in bounded frames, following the discipline already
proven by `StreamReadHostDocument`:

```proto
rpc StreamExecuteTool(ExecuteToolRequest) returns (stream ExecuteToolChunk);

message ExecuteToolChunk {
  bytes  result_chunk   = 1;  // successive slices of result_json's UTF-8 bytes
  bool   is_error       = 2;  // set on the final frame
  string error_message  = 3;
  string job_id         = 4;
  bool   job_running    = 5;
  bool   last           = 6;  // final frame marker
}
```

- Frame budget `EXEC_TOOL_FRAME_BYTES = 48 KiB`, pinned by a compile-time assert against
  `MAX_CHUNK_FRAME_BYTES` with envelope headroom — the same guard as `HOST_DOCUMENT_FRAME_BYTES`.
- Cross-host it rides `forward_server_stream_to_peer`, which terminates a stalled stream as an
  **error** rather than a clean end, so a truncated tool result can never look complete.
- The unary `ExecuteTool` is unchanged and still serves the stdio and HTTP paths.

## Behavior

### Session start

1. `StartSession` on A validates the split request: `managed_codebase` must be true, `session_type`
   must be `claude-cli`, and `codebase_daemon_instance_id` must name an eligible daemon in the
   common room. Any failure is a request error before anything is created.
2. A calls `StartSession` on B over the existing peer-forward path with `session_type: "workspace"`,
   the same `project_id`, and the branch/worktree intent from the original request. B resolves the
   project (auto-provisioning by clone if it does not have it yet) and creates the worktree.
3. A mints a scoped LiveKit join token for the session and spawns the agent with the
   `TDDY_REMOTE_*` env above, a read-only context directory, and an allowlist that excludes every
   native filesystem tool.
4. A writes `.session.yaml` recording `codebase_daemon_instance_id` and `codebase_session_id`, with
   `repo_path: None`.

If step 2 or 3 fails, the whole start fails and any worktree already created on B is removed — no
half-built split session is left behind.

### Agent working directory

There is no repository on A, so the agent's cwd is the read-only context directory that
managed-codebase mode already builds (`RemoteContextDir`): synced `CLAUDE.md` / `AGENTS.md` /
skills plus the `REMOTE_APPENDIX` notice telling the agent the real codebase is elsewhere and
reachable only through `mcp__tddy-tools__*`.

### Tool dispatch

Each tool call becomes one `StreamExecuteTool` to `daemon-{B}`, addressed at B's RPC-server
participant identity. B resolves the worktree from the **B-side** workspace session id and its own
sessions base, then runs `tool_engine::execute_tool` against it — the same handler that serves
co-located managed sessions.

**Long-running tools.** A forwarded stream is terminated after
`PEER_FORWARD_STREAM_IDLE_TIMEOUT` (30 s) without a frame. A tool whose result cannot begin within
that window must be driven through the existing background-job protocol — `Shell` with
`block_until_ms: 0` returns a `job_id` immediately, and `Await` is called in sub-deadline slices —
rather than one long blocking call. This is a constraint on the client, not a new keepalive
mechanism.

### Resume

A split session's `.session.yaml` has no `repo_path`, and the `TDDY_REMOTE_*` env was injected at
spawn time. Resume must therefore re-derive the remote wiring from the persisted
`codebase_daemon_instance_id` / `codebase_session_id` and mint a fresh join token — it cannot
reuse the original, which is scoped to a lifetime that may have elapsed.

`resume_claude_cli_session` currently rebuilds `env_extra` only from the persisted recipe and takes
its worktree from `meta.repo_path`. A split session resumed through it today would lose its tool
transport entirely and fail on the missing worktree.

### Teardown

`DeleteSession` on A deletes the paired workspace session on B before removing A's own session
directory. A failure to reach B fails the delete with a message naming the orphaned worktree — it
does not silently drop the B-side.

**This requires fixing an existing gap.** `session_deletion.rs` gates worktree removal on
`session_type == "claude-cli"` only:

```rust
let claude_cli_worktree = metadata
    .as_ref()
    .filter(|m| m.session_type.as_deref() == Some("claude-cli"))
    .and_then(|m| m.repo_path.clone());
```

So deleting a `workspace` session removes its session directory but leaves the git worktree and its
`git worktree` registration behind — despite
[remote-codebase-mode.md](remote-codebase-mode.md) criterion 3 asserting otherwise. The filter must
widen to include `"workspace"`, and the local binding and its log strings renamed off `claude_cli_`.
Without this, every split session leaks a worktree on B.

`cursor-cli` sessions leak the same way. That is a pre-existing bug outside this changeset's scope
— widening the filter for it would change behaviour for sessions this feature does not touch — and
is recorded in `docs/dev/TODO.md` instead.

### Preconditions

Both daemons must already share `livekit.api_secret` and the same `livekit.common_room` — the
former because session tokens are stateless HMACs verifiable only by daemons holding the same
secret, the latter because peer routing and the tool RPC both ride that room. The authenticated
GitHub user must map to an OS user on **both** daemons; B runs the tools as its own mapped user.

### Trust model

Unchanged from the existing multi-host model: any participant able to join the common room is an
eligible daemon. A split session lets an agent on A run arbitrary `Shell` in a worktree on B as B's
mapped OS user. That is the same authority `AddProjectToHost` and forwarded `StartSession` already
confer; this feature exposes it through the session form rather than adding new authority. The
scoped join token narrows only what the *agent process* holds — it cannot join other rooms or
impersonate another participant.

## Web UI

`CreateSessionPane` gains a **Codebase host** `<select>` inside the **claude-cli** managed-codebase
block, sourced from the same shared common-room daemon list as the existing Host selector.

- Rendered only when `sessionType` is `claude-cli`, `managedCodebase` is checked, and the common
  room advertises at least one daemon — so single-daemon usage is unchanged, and the control never
  appears where the request would be refused.
- Defaults to **"Same as host"** (empty value) — co-located, today's behaviour.
- Choosing a different daemon threads `codebaseDaemonInstanceId` into `StartSession`.
- Unchecking **Managed codebase** clears the selection, so the invalid combination cannot be sent,
  matching the existing convention where `specializedAgents` and `semanticIndex` are sent empty
  unless the block is open.

- Not offered in **peer mode** either. That flow joins an orchestrator's existing worktree and is
  given it as `repoPath`, so the codebase's location is already settled by the session being
  joined — the pane hides its Host and Project pickers for the same reason.

Note for implementation: the managed-codebase block exists **twice** in `CreateSessionPane` — once
in the cursor-cli branch and once in the claude-cli branch — sharing the same state and the same
`data-testid`s, so only one is mounted at a time. The new selector goes in the **claude-cli copy
only**. Because both copies share state, the claude-cli branch must also send an empty
`codebaseDaemonInstanceId` when the session type is cursor-cli, so a value chosen before switching
type cannot leak into a request that would be refused. One predicate (`isSplitCodebase`) governs
the selector's visibility, the recipe withdrawal, and what the request carries, so the three cannot
drift apart.

The sessions list renders a split session's placement as agent host and codebase host, sourced from
the new `SessionEntry` fields rather than inferred from which daemon answered.

## Non-goals

- **Web observability of the B side.** Long-lived streams (`StreamSessionActivity`,
  `StreamAcpReplay`, `StreamTerminalOutput`) still do not cross daemons; the agent's own terminal
  and activity live on A and are unaffected. Watching B's worktree from the web is out of scope.
- **Multi-hop.** A addresses B directly; no chains of intermediate daemons.
- **Migrating the stdio or HTTP tool paths to `StreamExecuteTool`.** They keep the unary call.
- **A generic RPC router.** Each cross-host RPC remains a hand-written arm.
- **Splitting `cursor-cli` or `tool` (tddy-coder) sessions.** Only `claude-cli` in v1; see
  § Why claude-cli only.
- **Fixing the pre-existing `cursor-cli` worktree leak** in `session_deletion.rs`. Recorded in
  `docs/dev/TODO.md`.
- **Making `tddy-coder --remote` work.** Its bootstrap is still unimplemented
  (`run.rs:4000-4003`); this feature delivers the daemon/UI path instead and leaves the CLI
  entry point as it is.
- **Retiring `tddy-daemon --relay`.** The CLI `tddy-coder --remote` path keeps its relay; this
  feature does not change it.

## Related documentation

- [Remote-codebase mode](remote-codebase-mode.md) — the tool-proxy model this extends
- [Managed-codebase mode + discovery subagents](../coder/managed-codebase-subagents.md)
- [Projects screen & multi-host projects](../web/projects-screen-multi-host.md) — `AddProjectToHost`, auto-provisioning
- [Daemon selector + LiveKit-only RPC routing](../web/daemon-selector-livekit-rpc.md) — the shared daemon list the new selector reuses
- [RPC multi-transport](../coder/rpc-multi-transport.md) — why LiveKit is just another `tddy-rpc` binding
- [LiveKit peer discovery and host selection](livekit-peer-discovery.md) — `forward_to_peer`, routing, trust
