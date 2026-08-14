# Changeset: Remote Managed Worktree

**PRD**: `docs/ft/daemon/remote-managed-worktree.md`
**Branch**: `feat-remote-managed-repo`

## Checklist

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Write acceptance tests
- [x] Write unit tests
- [ ] Implement proto + generated types
- [ ] Implement `SessionToolTransport::LiveKit` in tddy-tools
- [ ] Implement `StreamExecuteTool` + frame budget
- [ ] Implement split placement in `StartSession`
- [ ] Implement paired teardown + workspace worktree removal
- [ ] Implement resume for split sessions
- [ ] Implement the web codebase-host selector
- [ ] Record out-of-scope findings in `docs/dev/TODO.md`

## State A (current behaviour)

- `StartSessionRequest.daemon_instance_id` (`connection.proto:391`) is the **only** host axis: it
  places the agent and its worktree together. Highest tag in use is 31; `reserved 19..23`.
- `managed_codebase` (field 17) reaches the daemon as `_managed_codebase`
  (`connection_service.rs:2459`) — the sandboxed path never mounts the repo regardless
  (`mounts: vec![]`), so the flag only selects a workflow recipe (`:4840-4850`).
- `ExecuteTool` already peer-forwards over LiveKit (`connection_service.rs:7297-7343`), resolving
  the worktree from **the serving daemon's own** sessions base via `req.session_id`
  (`:7362-7363` → `workspace_session::resolve_worktree_root_for_session`).
- `session_type: "workspace"` exists and creates a real git worktree with no PTY and no agent
  (`workspace_session.rs:19-129`).
- `SessionToolTransport` has two variants, `SandboxIpc` and `DaemonHttp`
  (`session_tool_client.rs:9-20`). `dispatch_via_stdio_rpc` is already transport-agnostic — it
  takes `Arc<dyn tddy_rpc::RpcClientTransport>` — but sends an **empty** `session_token` /
  `session_id` / `daemon_instance_id` (`:198-204`), because the sandbox socket implies identity.
- `tddy_livekit::RpcClient` already implements `RpcClientTransport` (`client.rs:304`), and
  `tddy-tools` already has an optional `tddy-livekit` dependency behind a `livekit` feature used by
  `pty_relay.rs`.
- `RemoteToolEnv` (`tddy-core/src/backend/mod.rs:356`) carries `livekit_url`, `livekit_room` and
  `server_identity`, all **always `None`** — only `tddy-coder`'s CLI `RemoteConfig` builds one
  (`tddy-coder/src/config.rs:139`), and no daemon spawn path passes `--remote*`.
- `SessionMetadata` (`tddy-core/src/session_metadata.rs:12`) has no daemon field at all;
  `SessionEntry.daemon_instance_id` is stamped at read time (`connection_service.rs:5391`, `:5409`).
- `session_deletion.rs:166-169` gates worktree removal on `session_type == "claude-cli"` only, so
  `workspace` sessions leak their worktree — contradicting `remote-codebase-mode.md` criterion 3.
- `resume_claude_cli_session` (`connection_service.rs:3435`) rebuilds `env_extra` only from the
  persisted recipe, and takes its worktree from `meta.repo_path`.
- `MAX_CHUNK_FRAME_BYTES = 60_000` (`tddy-livekit/src/chunking.rs:31`); reassembly is best-effort
  and index-keyed, so a lost frame wedges a call with no error.
  `PEER_FORWARD_TIMEOUT` and `PEER_FORWARD_STREAM_IDLE_TIMEOUT` are both 30 s
  (`livekit_peer_discovery.rs:1087`, `:1094`).
- `CreateSessionPane.tsx` has the Host `<select>` at `:637-658` and the managed-codebase block
  **twice** — cursor-cli `:790-864`, claude-cli `:934-1011` — sharing state and `data-testid`s.

## Files to create

| File | Purpose |
|------|---------|
| `packages/tddy-daemon/tests/remote_managed_worktree_acceptance.rs` | Placement validation, no LiveKit (mirrors `relay_peer_forwarding_acceptance.rs`) |
| `packages/tddy-daemon/tests/remote_managed_worktree_cross_host_acceptance.rs` | Two real daemons over the LiveKit testkit |
| `packages/tddy-daemon/tests/stream_execute_tool_acceptance.rs` | Frame budget and multi-frame reassembly |
| `packages/tddy-daemon/tests/workspace_session_deletion_acceptance.rs` | Workspace worktree removal |
| `packages/tddy-tools/tests/session_tool_livekit_dispatch.rs` | LiveKit transport detection, `Await` clamping, request envelope |
| `packages/tddy-daemon/tests/worktree_removal_eligibility.rs` | Which session types own a removable worktree |
| `packages/tddy-core/tests/split_session_env_acceptance.rs` | `RemoteToolEnv` → `TDDY_REMOTE_*` for a split session |
| `packages/tddy-core/tests/split_session_metadata_acceptance.rs` | `.session.yaml` persistence of the pairing |
| `packages/tddy-web/cypress/component/CreateSessionCodebaseHostAcceptance.cy.tsx` | Codebase-host selector behaviour |
| `packages/tddy-web/cypress/component/SessionsSplitPlacementAcceptance.cy.tsx` | Split placement rendering in the session list |

## Files to modify

| File | Change |
|------|--------|
| `packages/tddy-service/proto/connection.proto` | `StartSessionRequest.codebase_daemon_instance_id = 32`; `SessionEntry.codebase_daemon_instance_id = 29`, `codebase_session_id = 30`; `StreamExecuteTool` rpc + `ExecuteToolChunk` |
| `packages/tddy-core/src/session_metadata.rs` | `codebase_daemon_instance_id`, `codebase_session_id`; both `Option<String>`, `skip_serializing_if` |
| `packages/tddy-core/src/backend/mod.rs` | `RemoteToolEnv.livekit_token`; `env_pairs()` emits `TDDY_REMOTE_LIVEKIT_TOKEN` |
| `packages/tddy-tools/src/session_tool_client.rs` | `SessionToolTransport::LiveKit`; detection precedence; generalize dispatch to carry the request envelope |
| `packages/tddy-tools/Cargo.toml` | Make the `livekit` feature default, or enable it for the MCP path |
| `packages/tddy-daemon/src/connection_service.rs` | `classify_codebase_placement`; split branch in `start_session_core`; `stream_execute_tool`; split-aware `resume_claude_cli_session`; paired `delete_session` |
| `packages/tddy-daemon/src/livekit_peer_discovery.rs` | `forward_stream_execute_tool_via_livekit` wrapper |
| `packages/tddy-daemon/src/session_deletion.rs` | Widen worktree removal to include `"workspace"`; rename `claude_cli_worktree` and its log strings |
| `packages/tddy-daemon/src/workspace_session.rs` | Accept branch/worktree intent from the originating request |
| `packages/tddy-web/src/components/sessions/CreateSessionPane.tsx` | Codebase-host `<select>` in the claude-cli managed block; thread `codebaseDaemonInstanceId` |
| `packages/tddy-web/cypress/support/testIds.ts` | `createSessionCodebaseHostSelect` |
| `packages/tddy-web/cypress/support/pages/createSessionPage.ts` | `selectCodebaseHost`, `codebaseHostOptionValues`, `codebaseHostIsAbsent` |
| `docs/dev/TODO.md` | Out-of-scope findings (below) |
| `docs/ft/daemon/remote-codebase-mode.md` | Cross-link; correct criterion 3's worktree-removal claim |

## Design decisions

### Placement classification is a pure function

Mirroring `classify_peer_route`, validation lives in a pure, unit-testable function rather than
inline in `start_session_core`:

```rust
pub enum CodebasePlacement {
    CoLocated,
    Split { codebase_instance_id: String },
}

pub fn classify_codebase_placement(
    local_instance_id: &str,
    requested_codebase_id: &str,
    eligible_ids: &[String],
    managed_codebase: bool,
    session_type: &str,
) -> Result<CodebasePlacement, String>
```

Empty or self-matching → `CoLocated`. Otherwise every precondition (`managed_codebase`,
`session_type == "claude-cli"`, membership of `eligible_ids`) is checked and reported by name. This
keeps the expensive cross-host tests focused on wiring rather than on validation branches.

### `TDDY_REMOTE_SESSION_ID` carries the B-side id

B's `ExecuteTool` resolves the worktree from its *own* sessions base keyed by `req.session_id`. The
agent must therefore be given the **workspace session id on B**, not its own session id on A. This
is the single easiest thing to get wrong, so it gets a dedicated acceptance test.

### Reuse `TokenGenerator`, do not pass the API secret

`tddy_livekit::TokenGenerator` already emits the exact grants needed, including the
`can_publish_data` the RPC data channel requires. The daemon mints per-session with a scoped
identity. We explicitly do not follow `spawner.rs:886-902`, which passes `--livekit-api-secret` on
the child command line where `/proc/<pid>/cmdline` exposes it.

### `StreamExecuteTool` is additive

The unary `ExecuteTool` is untouched and keeps serving the stdio and HTTP paths. Only the LiveKit
transport uses the streaming variant. `EXEC_TOOL_FRAME_BYTES = 48 KiB` is pinned by a compile-time
assert against `MAX_CHUNK_FRAME_BYTES` with envelope headroom, exactly as
`HOST_DOCUMENT_FRAME_BYTES` is (`connection_service.rs:9690`, `:9705-9707`).

### Long-running tools use the existing job protocol, and the client enforces it

A forwarded stream dies after 30 s without a frame. Rather than introduce keepalive frames, tools
that cannot produce a first frame in time are driven through `block_until_ms: 0` → `job_id` →
`Await` in sub-deadline slices.

Stated as prose this is only a convention, and a convention nothing checks is a latent bug: an agent
asking `Await` to block for five minutes would surface as a *transport* failure, which is the
hardest kind of error to attribute. So the client clamps it —
`clamp_await_block_ms(requested) -> u64`, bounded by `MAX_REMOTE_AWAIT_BLOCK_MS`, which a test pins
below the forwarded-stream deadline with headroom for the round trip. Clamping is a ceiling only:
`0` stays non-blocking, and a short poll is left alone.

### Failure is atomic

If the workspace session on B is created but the agent spawn on A fails, the B-side is torn down
before the error returns. No half-built split session survives a failed start.

### The tool dispatch seam is the transport, not LiveKit

`dispatch_via_stdio_rpc` already accepts an `Arc<dyn RpcClientTransport>`, and
`tddy_livekit::RpcClient` already implements it — so no LiveKit-specific dispatch is needed. What the
function lacks is the **request envelope**: it hardcodes an empty `session_id` / `session_token` /
`daemon_instance_id`, which is correct only for the sandbox socket, where identity is implied. It is
renamed to `dispatch_via_rpc_transport` and takes a `SessionToolEnvelope`.

That also settles how to test it: the transport is injectable, so the envelope and round-trip tests
run against an **in-process duplex peer** rather than a spawned LiveKit fixture binary. Only the
detection of `SessionToolTransport::LiveKit` is LiveKit-specific, and that is pure env parsing. The
real wire is covered once, end to end, by the cross-host daemon suite — there is no value in paying
for a LiveKit container twice.

## Risks

- **LiveKit testkit grants.** `LiveKitTestkit::generate_token` sets
  `room_join`/`can_publish`/`can_subscribe`/`can_update_own_metadata` but not `can_publish_data`,
  unlike production `TokenGenerator`. Existing cross-host tests pass, so the data channel evidently
  works — but if a new LiveKit dispatch test fails to publish, this is the first thing to check.
- ~~**`StreamExecuteTool` is implemented but nothing calls it.**~~ **Closed.** `tddy-tools`'
  LiveKit arm now calls `dispatch_via_streaming_rpc`, which reassembles `result_chunk` in arrival
  order and — the point of the whole exercise — treats a stream that ends without its `last` frame
  as an **error**, discarding the accumulated prefix rather than returning half a file that reads
  as a whole one. Same for a mid-stream error status. The output shape is byte-identical to the
  unary path, so an agent cannot tell the transports apart. `SandboxIpc` and `DaemonHttp` keep the
  unary call: neither crosses LiveKit chunk framing.
- **Every remote tool call currently pays a full LiveKit room connect.** `dispatch_via_livekit`
  connects a room, waits for the codebase daemon's participant, issues one `ExecuteTool`, and drops
  the room — per call. An agent doing fifty `Read`s pays fifty connects, likely hundreds of
  milliseconds each. This is the single biggest threat to the feature being usable rather than
  merely correct, and it is **not** covered by any test: the suite drives the transport-agnostic
  dispatch through an in-process peer, so the connect cost is invisible to it.
  Fixing it means caching one room per process, which needs reconnect-on-drop semantics — a
  fallback decision, so it was deliberately left alone (`session_tool_client.rs`, marked TODO).
  Measure it against a real daemon before calling this feature done.
- **`tddy-tools` binary size / feature gating — resolved, with a cost.** `livekit` is now a
  **default** feature, because nothing in the repo builds `tddy-tools` with `--features livekit`
  (`./release` and `./test` both use default features), so leaving it opt-in would ship a binary
  whose split-session dispatch always returned "requires the 'livekit' cargo feature".
  Measured: debug binary **185.8 MB** with default features vs **123.4 MB** with
  `--no-default-features` — **+62.4 MB** of statically linked libwebrtc and debuginfo. Release
  /stripped was not measured. An in-jail build that only needs stdio can drop it entirely with
  `--no-default-features`, which compiles and lints clean; the LiveKit arm then returns an explicit
  feature error rather than silently falling back to a transport pointed at the wrong host.
- **The cross-host suite has never been executed.** It is verified red at compile time only; running
  it needs the LiveKit testkit container (Docker or `LIVEKIT_TESTKIT_WS_URL`). Expect to debug the
  harness itself on first green run.
- **Validation order decides whether the teardown test means anything.** If every precondition is
  checked before the codebase host is contacted, an invalid request never creates a B-side and
  `a_failed_agent_spawn_tears_down_the_workspace_session_on_the_codebase_daemon` would pass without
  exercising teardown at all. It is written against an agent host whose `claude` binary does not
  exist, so the failure lands *after* B has created its workspace session — keep it that way.

### Merged with `#385` (deterministic test suite), and adopted its helpers

`origin/master`'s `#385` landed shared determinism helpers in `tddy-testing-commons` that this
branch had hand-rolled hours earlier. Both were replaced rather than left to drift:

- `wait_until_discovered` used `timeout` + `sleep(400ms)` and panicked with a bare message. It now
  uses `eventually_awaiting`, which polls at 25 ms and reports the eligible list it *did* see — for
  a peer that never appears, the difference between "timed out" and "these daemons were visible and
  yours was not".
- The claude stub was `/bin/cat`. `#385` documents the trap: `cat` treats a positional argument as
  a filename and exits, so the moment a fixture grew an `initial_prompt` the stub would die and the
  failure would read as a spawn bug. It is now
  `a_stub_agent_script(...).then_reading_stdin()`.

The one merge conflict of substance was `session_tool_stdio_rpc_dispatch.rs`: master rewrote how it
waits, this branch renamed the function it calls. Both kept — master's determinism work is better
than what the rename would have overwritten.

### Test-environment requirements found while writing the red phase

- **claude-cli sessions need a stub binary.** `claude_cli.binary_path` must point at something
  spawnable or every start fails reaching for `claude` on PATH, for reasons unrelated to placement.
  The cross-host fixtures use `/bin/cat` (blocks on stdin, the shape a PTY session needs), mirroring
  `claude_cli_session_acceptance.rs`. This is why the existing `session_attach_cross_host` suite
  uses `workspace` sessions throughout — they spawn nothing.
- **Worktree fixtures need an `origin` remote.** Worktree setup runs `git fetch origin`; a bare
  `git init` repo fails there before any behaviour under test is reached. Every repo fixture adds
  itself as `origin`.

## Acceptance tests

### Web — codebase-host selector

`packages/tddy-web/cypress/component/CreateSessionCodebaseHostAcceptance.cy.tsx`
Follows `CreateSessionHostSelectionAcceptance.cy.tsx` (page object, no raw `cy.get`), with
`anInMemoryRpcBackend` and `SelectedDaemonProvider`.

1. **`the codebase host selector is hidden until managed codebase is enabled`** — with claude-cli
   selected and the checkbox unchecked, the control is absent. Guards the PRD rule that split
   placement is meaningless without managed codebase.
2. **`the codebase host selector is hidden for cursor-cli sessions`** — enabling managed codebase on
   cursor-cli shows the recipe and subagent controls but no codebase host. Pins the v1 restriction
   in the UI, not just the daemon.
3. **`the codebase host defaults to running the codebase on the session host`** — the control's
   value is empty and its first option reads "Same as host". Pins that the default is co-located.
4. **`choosing a codebase host sends it in the start session request`** — selecting daemon B yields
   one `StartSession` with `codebaseDaemonInstanceId === "daemon-b"` and `managedCodebase === true`.
   The core behaviour.
5. **`leaving the codebase host as same as host sends an empty codebase daemon`** — asserts the
   co-located request is byte-identical to today's, so existing flows cannot regress.
6. **`switching to cursor-cli drops a previously chosen codebase host from the request`** — choose B
   on claude-cli, switch type, submit; the request carries an empty `codebaseDaemonInstanceId`.
   Guards the shared-state leak between the two duplicated managed blocks.
7. **`disabling managed codebase drops a previously chosen codebase host from the request`** — same
   guard along the other axis, matching the `semanticIndex` / `specializedAgents` convention.

### Web — split placement rendering

`packages/tddy-web/cypress/component/SessionsSplitPlacementAcceptance.cy.tsx`

8. **`a split session shows its agent host and its codebase host`** — a `SessionEntry` with
   `daemonInstanceId: "daemon-a"` and `codebaseDaemonInstanceId: "daemon-b"` renders both.
9. **`a co-located session shows a single host`** — an entry with an empty
   `codebaseDaemonInstanceId` renders one host and no second label.

### Daemon — placement validation (no LiveKit)

`packages/tddy-daemon/tests/remote_managed_worktree_acceptance.rs`
Uses `MockEligibleDaemonSource` and a `None` room slot, per `relay_peer_forwarding_acceptance.rs`.

10. **`start_session_with_a_codebase_daemon_but_without_managed_codebase_is_refused`** —
    `InvalidArgument`, message naming `managed_codebase`.
11. **`start_session_with_a_codebase_daemon_on_a_cursor_cli_session_is_refused`** —
    `InvalidArgument`, message naming the session type. Pins the v1 restriction.
12. **`start_session_with_an_unknown_codebase_daemon_is_refused`** — `InvalidArgument` naming the
    offending id and `livekit.common_room`, matching `classify_peer_route`'s error shape.
13. **`start_session_with_a_codebase_daemon_matching_the_local_instance_is_co_located`** — succeeds
    and creates **no** workspace session; `.session.yaml` has a `repo_path` and no pairing fields.
14. **`start_session_with_a_known_codebase_daemon_and_no_livekit_room_fails_precondition`** —
    `FailedPrecondition`, the known-peer/no-room split this harness is built around.

### Daemon — cross-host (two real daemons)

`packages/tddy-daemon/tests/remote_managed_worktree_cross_host_acceptance.rs`
Two-daemon standup per `multi_host_acceptance.rs`, including the mandatory
`ListEligibleDaemons` readiness poll before any assertion.

15. **`a split session creates a workspace session on the codebase daemon`** — after `StartSession`
    on A, B's `ListSessions` contains one `session_type: "workspace"` whose worktree exists on disk.
16. **`a split session records its pairing and holds no local repo path`** — A's `.session.yaml` has
    `codebase_daemon_instance_id`, a `codebase_session_id` equal to B's workspace session, and
    `repo_path: None`. Pins the persistence decision.
17. **`the agent receives the codebase session id, not its own`** — the spawned process's
    `TDDY_REMOTE_SESSION_ID` equals B's workspace session id. Guards the easiest mistake in the
    design.
18. **`a tool call from the agent host executes in the worktree on the codebase daemon`** —
    `Write` then `Read` through the split path round-trips, and the file exists in B's worktree on
    disk. End-to-end proof.
19. **`deleting a split session deletes the paired workspace session and its worktree`** — after
    `DeleteSession` on A, B has no such session and the worktree directory is gone.
20. **`a failed agent spawn tears down the workspace session on the codebase daemon`** — with an
    unspawnable tool path, the start fails and B has no orphaned workspace session. Pins atomicity.

### Daemon — workspace worktree removal

`packages/tddy-daemon/tests/workspace_session_deletion_acceptance.rs`

21. **`deleting a workspace session removes its git worktree`** — currently fails: the removal is
    gated on `claude-cli`. Also asserts `git worktree list` no longer lists it, so the registration
    is cleaned and not merely the directory.

### Daemon — StreamExecuteTool

`packages/tddy-daemon/tests/stream_execute_tool_acceptance.rs`

22. **`the exec tool frame budget leaves headroom under the livekit chunk limit`** — asserts
    `EXEC_TOOL_FRAME_BYTES` + envelope overhead `<= MAX_CHUNK_FRAME_BYTES`, the runtime companion to
    the compile-time assert.
23. **`a result larger than one frame arrives as ordered frames and reassembles exactly`** — reads a
    file well above the budget and compares the reassembly byte-for-byte against the file. The
    reason this RPC exists.
24. **`a streamed tool result equals the unary result for the same call`** — same tool, both RPCs,
    identical `result_json`. Pins that the streaming variant is a transport change, not a semantic
    one.
25. **`a tool error is reported on the final frame, not as a stream error`** — an unknown tool name
    yields `is_error: true` in the last frame, matching unary `ExecuteTool`'s contract that tool
    errors are results and only routing failures are RPC errors.

### tddy-tools — LiveKit transport

`packages/tddy-tools/tests/session_tool_livekit_dispatch.rs`, with
`tests/fixtures/execute_tool_livekit_fixture.rs` hosting a fake `ExecuteTool` over LiveKit.

26. **`detects the livekit transport when the livekit environment is configured`** — url, room,
    token, server identity and session id present → `SessionToolTransport::LiveKit`.
27. **`prefers sandbox ipc over livekit when both are configured`** — pins the documented detection
    precedence so an in-jail session never leaves the host.
28. **`dispatches a tool call over livekit rpc and returns the result json`** — round-trip against
    the fixture.
29. **`carries the session id and token in the livekit request envelope`** — the fixture echoes the
    decoded request; asserts non-empty `session_id`, `session_token`, `daemon_instance_id`. This is
    what distinguishes the LiveKit path from the sandbox path, which sends them empty.

## Unit tests

### `packages/tddy-daemon/src/connection_service.rs` — `classify_codebase_placement`

- `an empty codebase daemon is co located`
- `a codebase daemon matching the local instance is co located`
- `a known peer with managed codebase on a claude cli session is a split placement`
- `a split placement without managed codebase is rejected naming the flag`
- `a split placement on a cursor cli session is rejected naming the session type`
- `a split placement on a tool session is rejected naming the session type`
- `an unknown codebase daemon is rejected naming the id and the common room`

### `packages/tddy-core/src/backend/mod.rs` — `RemoteToolEnv`

- `env pairs include the livekit token when it is set`
- `env pairs omit the livekit token when it is absent`
- `env pairs include the livekit url room and server identity for a split session`

### `packages/tddy-core/src/session_metadata.rs`

- `a split session round trips its codebase daemon and codebase session id`
- `a co located session omits both codebase fields from the serialized yaml`
- `a legacy session file without codebase fields still parses`

### `packages/tddy-tools/src/session_tool_client.rs`

- `livekit transport requires url room token identity and session id`
- `a partial livekit environment falls through to the http transport`
- `no configured transport returns the not configured error`

### `packages/tddy-daemon/src/session_deletion.rs`

- `a workspace session is eligible for worktree removal`
- `a claude cli session is eligible for worktree removal`
- `a tool session is not eligible for worktree removal`

## Out-of-scope findings for `docs/dev/TODO.md`

- **cursor-cli cannot enforce managed codebase.** No `--allowedTools` equivalent exists; a split
  cursor session would be guidance-only. Would also need a context dir, MCP config, tool-relay env
  and resume env — `cursor_cli_spawn.rs:302` discards `managed_codebase` today.
- **cursor-cli sessions leak their worktree on delete.** Same root cause as the workspace leak
  (`session_deletion.rs:166-169`), but fixing it changes behaviour for sessions this changeset does
  not touch.
- **`tddy-coder --remote` bootstrap is unimplemented.** `run.rs:4000-4003` bails after contacting
  the relay. `RemoteContextDir` is referenced only from tests.
- **`remote-codebase-mode.md` criterion 3 is wrong** about `DeleteSession` removing a workspace
  worktree. Corrected as part of this changeset; noted because the same doc has other criteria
  asserting behaviour worth re-verifying.
- **`docs/ft/daemon/background-tasks.md:153-155` claims TaskService unary methods peer-forward.**
  They do not; `daemon_instance_id` is ignored there.
- **`TokenGenerator::generate_for` performs no authorization.** Any caller reaching `TokenService`
  can mint a token for any room and identity.
