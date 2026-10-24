# Initial Discovery: A stalled or impossible spawn says why

**Changeset**: [2026-10-24-start-progress-spawn-diag.md](./2026-10-24-start-progress-spawn-diag.md)
**Date**: 2026-10-24
**Passes**: 1 (the whole-work discovery of `#start-progress`, copied verbatim below; node-specific passes are appended after it)

## Combined Conclusions

See the whole-work Combined Conclusions inside Exploration 1. This node's slice of them is State A in the changeset.

## Exploration 1: `#start-progress` whole-work discovery — 2026-10-24

**Agent**: Explore subagents ×3 + parent Grep/Read ×3 (six passes, nested below)
**Scope**: the whole stack


### Combined Conclusions

### The incident (2026-10-24, desktop → `mantas-udoo-ros`)

A split claude-cli start (agent on the desktop daemon, codebase on `mantas-udoo-ros`) failed after
30 s. Four independent defects lined up:

1. **The remote daemon cloned through its forked worker, not the supervisor.** Its
   `/etc/tddy/daemon.yaml` predates the supervisor and has no `supervisor:` block; `./install` keeps
   an existing `daemon.yaml`, so the block that switches spawning to the supervisor was never added.
   `spawn_backend_choice` (`tddy-spawn/src/supervisor_client.rs:27`) therefore chose
   `ForkedWorker`, and the worker runs as the unprivileged `tddy` user — it can never become
   `mantas`.
2. **The clone child deadlocked instead of failing with EPERM.** `clone_as_user_with_env`'s
   `pre_exec` (`tddy-spawn/src/spawner.rs:707`) calls `std::env::set_var("HOME", …)`. `std`'s
   `Command::spawn` holds the environment read lock across `fork` and forgets the guard in the
   child; `set_var` takes the write lock → the child waits forever on a futex. Observed: PID
   257936, forked by the worker at 15:09:17, still uid `tddy`, never exec'd. The same closure
   exists in `spawn_as_user` (`spawner.rs:1269`) and in the daemon-kernel's `as_user_command`
   (`tddy-daemon-kernel/src/spawn_as_user.rs:214`, which runs in the **multi-threaded** daemon).
   The `set_var` is also pointless (`cmd.env("HOME")` is already set; `Command` installs its env
   after `pre_exec`). All three also call `initgroups(3)` after fork, which allocates and enters
   NSS — `tddy-supervisor`'s `spawn_broker` documents and avoids exactly this.
   **Not proven on the host** (no sudo for `/proc/<pid>/stack`); argued from std semantics.
3. **The 30 s was the peer's own clone timeout, misread as the forward deadline.** The desktop's
   split forward deadline was 330 s (`spawn_worker_request_timeout_secs: 300` +
   `peer_forward_timeout_secs: 30` in `~/.tddy/desktop.yaml`). The remote has
   `spawn_worker_request_timeout_secs: 30`, so `ensure_project_available_for_start`'s
   `tokio::time::timeout` fired at 30 s and answered `DEADLINE_EXCEEDED`. The desktop's
   `SplitStartFailure::from_forward_error` maps **any** `DeadlineExceeded` to `ForwardDeadline`, so
   it logged "the forwarded start had already timed out". The remote logged **nothing** when its
   timeout fired.
4. **After the timeout the worker is wedged for good.** The timeout abandons the blocking task;
   the worker is still blocked in `cmd.output()` on the hung child; every later spawn/clone request
   is written to the pipe and never read. Nothing kills the child or restarts the worker.

And one product gap: **the UI showed nothing for 30 s.** The split path holds the caller's
streaming sink but forwards a *unary* `StartSession` to the peer; the peer runs it with
`AttachmentProgressSink::discarding()`; the clone has no `StartPhase`; no clone passes
`--progress`; neither backend streams child output.

### Packages in play

| Package | Role in this change |
|---|---|
| `tddy-spawn` | clone/spawn execution (`spawner.rs`, `spawn_worker.rs`, `supervisor_spawn.rs`, `supervisor_client.rs`) — stage pipe, deadlock fix, fail-fast, `--progress`, streaming clone output, backend choice from the supervisor's handoff |
| `tddy-supervisor` | managed-service child environment (`spawn_broker.rs::ChildEnvironment`) — announces its socket to the daemon; clone RPC that streams git's stderr |
| `tddy-daemon-kernel` | `DaemonConfig` (`supervisor` removed, obsolete-key error), `spawn_as_user.rs` pre_exec, `peer_calls` / `peer_forwarding` (streamed forward with no overall deadline) |
| `tddy-daemon` | `main.rs` backend choice before tokio |
| `tddy-desktop` | `src-tauri/src/lib.rs` same backend choice |
| `tddy-agent-launch` | `svc_start_session_core.rs`, `start_request_checks.rs`, `svc_ensure_project_available_for_start.rs`, `clone_backend.rs`, `start_keepalive.rs`, `session_coordinate_handlers.rs` — clone phase + progress, timeout removal, cancel registry |
| `tddy-session-split` | `split_claude_cli_start.rs` (forward becomes streamed, peer events relayed), `split_start.rs` (failure classification), `workspace_session.rs` (worktree timeout), teardown |
| `tddy-session-agents` | `agent_roster.rs::split_forward_deadline`, `svc_provision_agent_clone.rs` (second unary user) |
| `tddy-session-files` | `AttachmentProgressSink` — new clone-progress event, cancellation awareness |
| `tddy-service` | `session.proto` — `STEP_CLONE`, `CloneProgress` event, `CancelStartSession` RPC |
| `tddy-projects` | `project_provision.rs` cloner signature `Fn(&str,&Path)->Result<(),String>` has no progress channel |
| `tddy-daemon-rpc` | `project/coordinate_handlers.rs` CreateProject/AddProjectToHost clone timeouts (shared code only) |
| `tddy-web` | `useSessionAttachments.ts`, `CreateSessionStartPhase.tsx`, `CreateSessionActions.tsx`, `useCreateSessionSubmit.ts` |
| `install`, `daemon.yaml.production`, `dev.supervisor.yaml` | strip `supervisor:` on upgrade; template and stale comments |

### Architecture facts the plan rests on

- **Backend choice** is one pure function of config, called pre-tokio in `tddy-daemon/src/main.rs:56`
  and `tddy-desktop/src-tauri/src/lib.rs:63`, and **re-derived from `self.config` at operation time**
  in five places (`tool_session_spawn.rs:188`, `svc_ensure_project_available_for_start.rs:155`,
  `coordinate_handlers.rs:123/277`, telegram `workflow_spawn.rs:271`). Replacing the config switch
  means injecting the startup decision into those call sites.
- **The supervisor already hands the daemon state through the environment**: `ChildEnvironment::build`
  (`spawn_broker.rs:1339-1405`) overlays per-service `env:`, forces HOME/USER/LOGNAME, strips
  `LISTEN_*` and re-adds them for the handover. A supervisor-set variable added **after** the `env:`
  overlay cannot be spoofed by `supervisor.yaml`. The privileged listener is bound before the first
  service starts, so a daemon connecting early queues in the backlog. The daemon user is in
  `tddy-clients`; authorization is by SO_PEERCRED uid ∈ declared service uids.
- **Supervisor protocol** is tddy-rpc frames over AF_UNIX (`StdioEndpoint::from_duplex`), all 11 RPCs
  unary. A supervised clone is `SpawnSession{git clone url dest}` + 25 ms `SessionStatus` polling;
  output goes to `/dev/null` (`inherit_output: false`); only the exit code returns. `stop_session(pid)`
  exists, so the supervised backend can kill a clone.
- **Forked worker protocol** is newline-JSON over two pipes, strictly one request at a time, no ids,
  no intermediate messages, no cancel. `SpawnClient::send_and_recv` releases the tx lock before taking
  the rx lock → two concurrent callers can read each other's responses. The worker is a fork without
  exec, single-threaded at birth but gains a detached waiter thread per spawned session.
- **`DaemonConfig` is `deny_unknown_fields`**; there is no obsolete-key message anywhere. `install`
  already has `drop_top_level_yaml_block FILE KEY` (lines 478-523), used only on freshly rendered files
  and matching only an exact `supervisor:` line.
- **Streamed forwarding exists** (`forward_stream_start_session` → `forward_server_stream_to_peer`):
  open deadline = `peer_forward_timeout`, but LiveKit's `call_server_stream` returns once published, so
  in practice the fixed 30 s `PEER_FORWARD_STREAM_IDLE_TIMEOUT` (`stream_relay::Relay`, reset by every
  frame) is the only bound. Only the whole-session forward uses it; the split forward and the
  agent-clone provision use the unary `forward_start_session_within(split_forward_deadline)`.
- **Keepalives** (`with_keepalive`, 5 s, `START_KEEPALIVE_INTERVAL`) wrap only
  `provision_project_for_start`. Nothing pings during the worktree cut, roster seed, semantic index
  or jail — each could exceed 30 s on a streamed hop.
- **Cancellation does not exist end to end**: the web LiveKit transport ignores `AbortSignal` on
  server streams (`envelope-transport.ts:291` TODO, `useSessionAttachments.ts:376`);
  `RpcRequest.abort` is on the wire but never set or read; `RpcService` has no cancellation surface
  (todo 2026-09-15); the server-side start runs on a detached `tokio::spawn`; the progress sink
  swallows send errors on purpose; the only server-side abort is per-participant disconnect, which
  does not stop `spawn_blocking`. Closest primitives: `TaskRegistry` + `CancellationToken` (warm-index
  precedent), supervisor `stop_session`, `DeleteSession`.
- **Start identity before completion**: `requested_session_id` (field 33) lets a caller name the
  session, but only `session_type = "workspace"` honours it.
- **The web form always streams** (`useCreateSessionSubmit.ts:85`); progress is one text line
  (`CreateSessionStartPhase.tsx`); the Cancel button is disabled while submitting.
- **`spawn_worker_request_timeout_secs` is far wider than clones**: session spawns, worktree-service
  RPCs (~10), session-file reads, PR-stack ports, ListProjectBranches, and `split_forward_deadline`.
  Removing the key entirely is a much wider change than removing it from the session-creation path.

### Constraints / conflicts

- No active `docs/dev/1-WIP/` changeset touches these files (listing checked: split-sandbox,
  livekit-rooms, restructure, dev-launchers, sandboxed-codebase, specialized-agent, subagent-turn,
  terminal-history).
- No open code issue in an affected package is claimed. Unclaimed issues in the path:
  `complexity-svc-ensure-project-available-for-start-…` (99 lines), `complexity-svc-start-session-core-…`
  (373), `complexity-svc-spawn-split-agent-…` (110, 9 params), `oversized-file-project-coordinate-handlers`
  (586), `oversized-file-livekit-peer-discovery` (1,407), `complexity-server-serve` (supervisor, 95).
  `tddy-spawn`, `tddy-daemon`, `tddy-rpc` have no `docs/code-issues/` (not analyzed).
- `docs/ft/daemon/remote-managed-worktree.md` § Failure is atomic justifies the 330 s split deadline;
  `docs/ft/daemon/clone-ssh-passphrase.md` pins "a forwarded start is bounded by the split-start
  budget" as an acceptance criterion. Both are superseded by this change.

### Open questions for the developer

1. `spawn_worker_request_timeout_secs` — remove only from the session-creation clone/worktree path
   (key stays for the ~20 other uses), or remove the key entirely?
2. Forked worker concurrency — the protocol serves one request at a time; a long clone blocks every
   session spawn on that host. In scope?
3. Supervisor clone progress needs a streaming supervisor RPC (git's stderr is `/dev/null` today).

### Exploration 1: The incident — local and remote logs — 2026-10-24

**Agent**: parent Bash (local files, `ssh mantas@raitininkai.ddnsgeek.com`)
**Scope**: why the session start timed out

### Sequence

1. `ls ~/.tddy ~/.tddy/logs` — find the desktop daemon's logs (`~/.tddy/logs/daemon`, rotated siblings).
2. `grep -vE 'synced N remote daemon' daemon | grep -E 'ERROR|WARN|timeout|StartSession|remote'` — errors around the start.
3. `awk '$1>="15:09:10" && $1<="15:10:00"'` on the desktop log — the start in full.
4. `ssh … systemctl status tddy-supervisor tddy-daemon; ls /var/log/tddy-daemon` — remote install shape.
5. Remote `awk` 15:08–15:11 on `/var/log/tddy-daemon/daemon` — the peer's side.
6. Remote `grep clone|askpass` across rotated logs; `ps` for git/ssh; `ls /home/mantas/repos`.
7. Read `packages/tddy-spawn/src/spawner.rs:600-744` (`clone_as_user_with_env`).
8. `git log -L705,720:spawner.rs` — the pre_exec block is old (since #77/#91).
9. Read `packages/tddy-agent-launch/src/clone_backend.rs`, `tddy-spawn/src/supervisor_client.rs:20-40`.
10. Remote `ps -o pid,ppid,user,lstart,stat,nlwp` for the three `tddy-daemon` processes.
11. Remote `grep` of `/etc/tddy/daemon.yaml` top-level keys; `cat /etc/tddy/supervisor.yaml`.
12. Read `install:615-640`, `daemon.yaml.production` `supervisor:` block.
13. `grep spawn_worker_request_timeout_secs|peer_forward_timeout_secs` on both hosts' configs.

### Grep / glob

| Tool | Pattern | Scope | Notable hits |
|---|---|---|---|
| Grep | `WARN\|ERROR\|timeout\|StartSession` | `~/.tddy/logs/daemon` | `15:09:17.609 classify_codebase_placement: codebase placed on peer instance_id=mantas-udoo-ros`; `15:09:47.831 [WARN] … did not have workspace session 01a16dd0-… at teardown time, but the forwarded start had already timed out`; recurring `aggregate_peer_project_entries: peer mantas-udoo-ros did not respond within 3s` |
| Grep | `clone\|spawn` | remote `/var/log/tddy-daemon/daemon*` | `15:09:17.874 SpawnClient: sending op=clone dest=/home/mantas/repos/tddy-coder os_user=mantas`; `clone_as_user: will run git clone as os_user=mantas`; **no** `clone_as_user ok` / `err` ever |
| Grep | top-level keys | remote `/etc/tddy/daemon.yaml` | `log, listen, web_bundle_path, auth_storage, github, telegram, spawn_worker_request_timeout_secs, livekit, users, allowed_tools, allowed_agents` — **no `supervisor:`** |
| Grep | timeouts | both configs | desktop: `spawn_worker_request_timeout_secs: 300`, `peer_forward_timeout_secs: 30`; remote: `spawn_worker_request_timeout_secs: 30` |

### Inspected files

#### Remote process table

```
    PID    PPID USER   STARTED                   STAT NLWP COMMAND
 257754  257741 tddy   Sat Oct 24 15:08:17 2026  Ssl    18 /usr/local/bin/tddy-daemon -c /etc/tddy/daemon.yaml
 257757  257754 tddy   Sat Oct 24 15:08:17 2026  S       1 /usr/local/bin/tddy-daemon -c /etc/tddy/daemon.yaml   # spawn worker
 257936  257757 tddy   Sat Oct 24 15:09:17 2026  S       1 /usr/local/bin/tddy-daemon -c /etc/tddy/daemon.yaml   # clone child, never exec'd
```

#### `packages/tddy-spawn/src/spawner.rs:704-721`

```rust
if !same_user {
    let home_dir_pre = home_dir.clone();
    unsafe {
        cmd.pre_exec(move || {
            std::env::set_var("HOME", &home_dir_pre);
            if libc::setgid(gid) != 0 { return Err(std::io::Error::last_os_error()); }
            if libc::initgroups(pw_name.as_ptr(), gid as _) != 0 { return Err(…); }
            if libc::setuid(uid) != 0 { return Err(…); }
            Ok(())
        });
    }
}
```

#### Remote log, 15:09:17 → 15:09:47

```
15:09:17.740 incoming call session.SessionService/StartSession request_id=45 (672 bytes) from=LT-R1VXTH2V6H-…
15:09:17.874 SpawnClient: sending op=clone dest=/home/mantas/repos/tddy-coder os_user=mantas
15:09:17.874 spawn_worker: calling clone_as_user os_user=mantas dest=/home/mantas/repos/tddy-coder
15:09:17.874 clone_as_user: will run git clone as os_user=mantas dest=/home/mantas/repos/tddy-coder
15:09:47.917 incoming call session.SessionService/DeleteSession request_id=46
15:09:47.917 delete_session_directory: no session directory on this daemon (session_id=01a16dd0-…); refusing as wrong routing / ownership
```

### Findings

- The peer's clone timed out at its own 30 s `spawn_worker_request_timeout_secs` and returned
  `DEADLINE_EXCEEDED` without a log line; the desktop classified it as its own forward deadline.
- The clone child is stuck pre-exec as `tddy`; the worker is wedged behind it.
- `daemon.yaml` on the remote lacks `supervisor:` because `install` preserved a pre-supervisor file.

### Exploration 2: Supervisor → daemon handoff, config key, supervisor clone protocol — 2026-10-24

**Agent**: Explore subagent
**Scope**: every reader/writer of `DaemonConfig.supervisor`; how the supervisor starts services; strict
keys; the supervisor clone op and wire protocol

### Sequence

1. `ls packages`.
2. Grep `SupervisorConfig|\.supervisor\b|supervisor\.socket_path|SUPERVISOR_SOCKET|TDDY_SUPERVISOR` in `packages/**/*.rs`.
3. Read `tddy-spawn/src/supervisor_client.rs` (79 lines).
4. Grep `supervisor` in `tddy-daemon-kernel/src`; read `config.rs:1-320`; grep `deny_unknown|load|fn`.
5. Read `config/daemon_methods.rs::load`; grep `obsolete|no longer|retired|has been removed`, `unknown field|unknown key`; read `tddy-sandbox-app/src/config.rs` retired-flag test.
6. Grep consumers `spawn_backend_choice|SpawnBackendChoice|spawn_worker_for|connect_supervisor|SupervisorClientConfig`.
7. Grep test construction `supervisor: Some|socket_path: PathBuf::from|tddy-supervisor.sock`.
8. List files with a `supervisor:` key; read the five daemon/desktop templates, `supervisor.yaml.production`, `dev.supervisor.yaml`.
9. `install`: grep, read lines 1-340, 380-640, 1055-1295.
10. Grep tddy-e2e tests for supervisor / socket_path / strip helper.
11. Grep docs; read `docs/ft/daemon/systemd-install.md:40-70`, `docs/ft/supervisor/tddy-supervisor.md`.
12. Read `tddy-supervisor/src/lib.rs`, `handover.rs`; grep `spawn_broker.rs`, `socket.rs`, `config.rs::ManagedService`.
13. Read `spawn_broker.rs:255-300, 1300-1430`, `supervisor.rs:846-882`, `socket.rs:1-110`.
14. Read `tddy-daemon/src/local_socket_server.rs:80-200`, `tddy-daemon/src/main.rs:1-120`, `tddy-agent-launch/src/inherited_host_sockets.rs:1-70`.
15. Read `tddy-supervisor/proto/supervisor.proto`, `protocol.rs:1-120`, `client.rs:1-120`, `server.rs:60-260, 425-610`, `authz.rs`.
16. Read `tddy-rpc/src/transport.rs:54-88`, `tddy-spawn/src/supervisor_spawn.rs:1-60, 116-135, 195-386`.
17. Read test configs (`supervisor_routing.rs`, `supervisor_spawn_delegation.rs`, `supervisor_clone_delegation.rs`, `create_project_acceptance.rs`), `tddy-desktop/src-tauri/src/lib.rs`, `daemon_config_service.rs:120-160`.

### Grep / glob

| Tool | Pattern | Scope | Notable hits |
|---|---|---|---|
| Grep | `SupervisorConfig\|\.supervisor\b\|…` | `packages/**/*.rs` | only reader `supervisor_client.rs:28,30`; `tddy-supervisor/src/config.rs:53` is a different type; `install_contract.rs:38,48` |
| Grep | `deny_unknown` | kernel `config.rs` | lines 59, 226, 303, 352, 373 |
| Grep | `obsolete\|retired…` | packages | no config obsolete-key message anywhere |
| Grep | `spawn_backend_choice\|…` | packages | `main.rs:56`, desktop `lib.rs:63`, `tool_session_spawn.rs:188`, `svc_ensure_project_available_for_start.rs:155`, `clone_backend.rs`, `coordinate_handlers.rs:123,277`, telegram `workflow_spawn.rs:271` |
| Grep | `supervisor:` key | tests | `supervisor_routing.rs:18`, `supervisor_spawn_delegation.rs:414`, `supervisor_clone_delegation.rs:35`, `create_project_acceptance.rs:106` |
| Grep | `supervisor` | templates | only `daemon.yaml.production` (7-11, 35-68) and `dev.daemon.yaml:144` (unrelated `supervisor_leaf`) |
| Grep | `supervisor\.socket_path…` | docs | `systemd-install.md:47,61`; `tddy-supervisor.service.example:66`; `tddy-daemon.service.example:10`; `daemon-kernel.md:353` |

### Inspected files

#### `packages/tddy-spawn/src/supervisor_client.rs`

```rust
pub enum SpawnBackendChoice { Supervisor { socket_path: PathBuf }, ForkedWorker }
pub fn spawn_backend_choice(config: &DaemonConfig) -> SpawnBackendChoice {
    match &config.supervisor {
        Some(supervisor) => SpawnBackendChoice::Supervisor { socket_path: supervisor.socket_path.clone() },
        None => SpawnBackendChoice::ForkedWorker,
    }
}
```

#### `packages/tddy-daemon-kernel/src/config.rs:186-230`

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub supervisor: Option<SupervisorClientConfig>,
…
#[serde(deny_unknown_fields)]
pub struct SupervisorClientConfig { pub socket_path: PathBuf }   // "Presence of this section is the switch … deliberately no fallback."
```

#### `packages/tddy-daemon-kernel/src/config/daemon_methods.rs:18-28`

```rust
let config: Self = serde_yaml::from_str(&contents)
    .map_err(|e| anyhow::anyhow!("failed to parse config {}: {}", path.display(), e))?;
config.session_room.validate().map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
```

#### `install:478-523` — `drop_top_level_yaml_block FILE KEY`

Removes a top-level `KEY:` line, its indented children and the comment/blank block directly above;
atomic write; asserts `! grep -q "^${key}:"` afterwards with a message hard-coded to the `--user` case.
Called only at `install:1109` on a freshly rendered file. An existing `daemon.yaml` is never modified
(`install:1114-1128` print a NOTE in supervised mode — stale text "no daemon code path reads this block
yet" — or a WARNING in `--user` mode). `install:1286-1288` cross-checks the socket path in `daemon.yaml`.

#### `packages/tddy-supervisor/src/spawn_broker.rs:1339-1405` — `ChildEnvironment::build`

Base env (Inherited for managed services, Minimal for sessions) → overlay `plan.env` → force
HOME/USER/LOGNAME → strip `LISTEN_*` → add `LISTEN_FDS`/`LISTEN_FDNAMES` (+ post-fork `LISTEN_PID`) for a
handover. Tests 2330-2443 pin that the supervisor's own env is kept out of sessions and that caller
`LISTEN_*` is stripped.

#### `packages/tddy-supervisor/src/lib.rs::run` order

load config → signals → `ForkBroker::start` → `Supervisor::new` → cgroup subtree →
`bind_privileged_listener` ("Bind before the first service starts") → reaper → `start_declared_services`
→ `tokio::spawn(server::serve(..))`.

#### `packages/tddy-supervisor/proto/supervisor.proto`

All unary: `ListServices`, `StartService`, `StopService`, `SpawnSession → SpawnedProcess{pid}`,
`SpawnSandbox`, `SessionStatus → {pid,state,exit_code}`, `StopSession`, `CreateScope`, `AttachPid`,
`DestroyScope`. Comment on `SpawnSandboxRequest`: "a path crosses a process boundary, a file descriptor
does not."

#### `packages/tddy-supervisor/src/server.rs`

`spawn_session` (68-82) → `resolve_spawn` → `ResolvedSpawn::plan` (236-258: `inherit_output: false`,
`environment: Minimal`) → pid returned immediately. `serve` (511-605): per-connection
`SupervisorServiceServer` over `StdioEndpoint::from_duplex`, "request/response in one direction only";
`authorize_peer()` on every RPC (uid ∈ declared service uids).

#### `packages/tddy-spawn/src/supervisor_spawn.rs`

- `clone_request_with_env` (116-135): `SpawnSessionRequest { tool_path: git, args: ["clone", url, dest], env, working_dir: None, scope: None }` — no `--progress`.
- `clone_repo_via_supervisor_with_env` (299-345): exists-check → `resolve_git_program()` → connect →
  `spawn_session` → `wait_for_exit` (25 ms `session_status` polling, unbounded; "how long a clone may
  take is the caller's deadline") → `"git clone failed: exited with code {code}"`. No stderr available.

#### `packages/tddy-rpc/src/transport.rs:54-88`

Frame = 4-byte BE length + 1-byte `FrameKind {Request, Response}` + encoded `RpcRequest`/`RpcResponse`.
`StdioEndpoint` is full-duplex and tddy-rpc supports server streaming elsewhere, so a streaming
supervisor RPC is possible on this transport; none is declared.

### Findings

- The only runtime reader of the key is `spawn_backend_choice`; five operation-time call sites re-derive
  it from `self.config` and need the startup decision injected instead.
- The supervisor can announce itself through `ChildEnvironment::build` with a variable added after the
  `env:` overlay (unspoofable from `supervisor.yaml`); an inherited connected fd conflicts with the
  documented per-operation connection model.
- The daemon decides the backend pre-tokio; any marker must be read there, and must not collide with
  `local_socket_server`'s later `remove_var` of `LISTEN_*`.
- No obsolete-key precedent; `session_room.validate()` is the "refused by name, prefixed with the path"
  precedent; kernel `config.rs:842-866` (`a_daemon_loaded_from`) is the test template.
  `daemon_config_service::write_config` re-serializes the whole struct.
- Supervised clone output is discarded and there is no streaming in the supervisor protocol.
- Stale text: `install:1116-1119`, `dev.supervisor.yaml:21-23` ("no daemon code path reads this block yet").

### Exploration 3: Clone execution — worker, pre_exec, timeouts, progress — 2026-10-24

**Agent**: Explore subagent
**Scope**: pre_exec safety, worker protocol and fork model, every clone/worktree timeout, clone env and
output, test seams, existing git-progress parsing

### Sequence

1. `find packages/tddy-spawn -type f` + `wc -l` (spawner.rs 2287, spawn_worker.rs 648, clone_env.rs 184, supervisor_spawn.rs 386, supervisor_client.rs 79, spawn_worker/clone_request.rs 36).
2. Grep `pre_exec` across `*.rs`.
3. Grep `spawn_worker_request_timeout_secs|spawn_blocking_with_timeout|request_timeout` (code + md).
4. Grep `Receiving objects|Resolving deltas|--progress|Counting objects`.
5. Read `spawner.rs:1-900, 1180-1380, 1745-1860`; `spawn_worker.rs` (all); `clone_request.rs`; `clone_env.rs`; `lib.rs`; `Cargo.toml`.
6. Read `supervisor_client.rs`, `supervisor_spawn.rs` (all); `tddy-daemon-kernel/src/spawn_as_user.rs` (all).
7. Read `tddy-sandbox-darwin/src/spawn.rs:60-120`, `tddy-sandbox-cgroups/src/lib.rs:120-175, 270-320` (+ `enter_rootless_jail`, `apply_bind_mounts`).
8. Read `tddy-supervisor/src/spawn_broker.rs:1-200, 320-900, 1300-1345`.
9. Grep clone API callers; read `clone_backend.rs`, `workspace_session.rs`, `svc_ensure_project_available_for_start.rs`, `clone_planner.rs`, `clone_ssh_env.rs`.
10. Read `coordinate_handlers.rs:1-380`; `tddy-session-split/src/service_util.rs:140-290`.
11. Read `config.rs` default (300), `daemon_methods.rs:98-110`, `tddy-daemon/src/main.rs:1-120`, `tddy-desktop/src-tauri/src/lib.rs:40-90`, `tddy-log/src/log_backend.rs:630-700`.
12. Read `start_request_checks.rs:140-190`, `svc_start_session_core.rs:235-290`, `workspace_branch_start.rs:15-125`.
13. Grep `AttachmentProgressSink`; read `attachment_progress.rs:1-130`, `start_keepalive.rs`, `session.proto:104-121, 432-447`.
14. Grep tests touching the worker/clone; read `tddy-agent-launch/tests/support/rig.rs:1-140`, heads of `tddy-spawn/tests/clone_request_env.rs`, `clone_env.rs`, `supervisor_routing.rs`; `create_project_acceptance.rs:1-150`; `supervisor_clone_delegation.rs:1-60`.
15. Read `tddy-worktree-service/src/remote_git_service.rs:580-720` (`GitChildRelay`) and its acceptance test 440-480.
16. Read `tddy-projects/src/project_provision.rs` cloner signatures; askpass files; `agent_roster.rs:170-190`.

### Grep / glob

| Tool | Pattern | Scope | Notable hits |
|---|---|---|---|
| Grep | `pre_exec` | `*.rs` | `spawner.rs:707` (clone), `spawner.rs:1269` (spawn_as_user), `tddy-daemon-kernel/src/spawn_as_user.rs:214`, `tddy-sandbox-darwin/src/spawn.rs:83` (setpgid, safe), `tddy-sandbox-cgroups/src/lib.rs:139,287`, `tddy-supervisor/src/spawn_broker.rs:531` (designed safe) |
| Grep | non-test `set_var(` | packages | `spawn_as_user.rs:215`, `spawner.rs:708,1270` (all inside pre_exec) |
| Grep | `--progress\|Receiving objects…` | packages | only a literal in `remote_git_relay_acceptance.rs:463,474`; **no parsing, no clone passes `--progress`** |
| Grep | `spawn_worker_request_timeout\|spawn_blocking_with_timeout\|request_timeout` | code | clone: `coordinate_handlers.rs:121,138,275,292`, `svc_ensure_project_available_for_start.rs:113,188`; worktree: `workspace_session.rs:112,211`, `service_util.rs:159,245`; plus ~20 non-clone uses (worktree-service RPCs, session spawns, session files, PR stack, `split_forward_deadline`) |
| Grep | `fork_spawn_worker\|SpawnClient` | tests | none fork the real worker; no non-root user-switch test |

### Inspected files

#### `packages/tddy-spawn/src/spawn_worker.rs`

```rust
#[serde(tag = "op", content = "payload")]
pub enum WorkerRequest { #[serde(rename="spawn")] Spawn(Box<SpawnRequest>), #[serde(rename="clone")] Clone(CloneRequest) }
#[serde(tag = "op")]
pub enum WorkerResponse { #[serde(rename="spawn_ok")] SpawnOk{result: SpawnResult}, #[serde(rename="clone_ok")] CloneOk, #[serde(rename="error")] Error{message: String} }
pub struct SpawnClient {
    request_tx: Arc<std::sync::Mutex<std::fs::File>>,
    response_rx: Arc<std::sync::Mutex<BufReader<std::fs::File>>>,
}
```

`send_and_recv` (179-215) writes under the tx lock, releases it, then blocks on `read_line` under the rx
lock. `fork_spawn_worker` (236-288): two `pipe()`s, `fork()` with no exec. `spawn_worker_main`
(296-433): `for line in reader.lines()` — one request at a time; no heartbeat, ids or cancel.

#### `packages/tddy-spawn/src/spawner.rs:688-736` (clone command and output)

```rust
let mut cmd = std::process::Command::new("sh");
cmd.arg("-c").arg(r#"if test -e "$1"; then exit 0; fi; exec git clone "$2" "$1""#)
    .arg("sh").arg(dest_str).arg(git_url)
    .env("HOME", &home_dir)
    .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
…
let output = cmd.output().map_err(|e| anyhow::anyhow!("git clone: {}", e))?;
```

#### `packages/tddy-supervisor/src/spawn_broker.rs` (reference design)

Header 1-44: "The group list is enumerated *before* the fork, so this is three bare syscalls;
`initgroups(3)` would allocate and enter NSS, neither of which is safe after a fork." `TargetUser { uid,
gid, name: CString, home, groups }`; `pub fn resolve_target_user(name)` (refuses root credentials;
`getgrouplist` pre-fork). `pre_exec_plan(&SpawnPlan) -> Vec<PreExecStep>` — steps as data, testable on
any host. 838-842: "the child cannot explain itself. `std` carries a `pre_exec` failure back to the
parent as an errno and nothing else." 1316: `Command` installs its env only after `pre_exec`.

#### `packages/tddy-agent-launch/src/svc_ensure_project_available_for_start.rs:113,188-194`

```rust
let timeout = self.config.spawn_worker_request_timeout();
…
match tokio::time::timeout(timeout, handle).await {
    Ok(Ok(res)) => res, Ok(Err(join_err)) => Err(Status::internal(..)),
    Err(_elapsed) => Err(Status::deadline_exceeded("ensure_project_available_for_start: clone timed out")),
}
```

No log line on expiry; the blocking task is abandoned.

#### `packages/tddy-session-split/src/service_util.rs:179-205` — `spawn_blocking_with_timeout`

Logs `"{} timed out after {}s (spawn_worker_request_timeout_secs); blocking task may still run in the
pool"` and returns `DEADLINE_EXCEEDED`; does not cancel.

#### `packages/tddy-projects/src/project_provision.rs`

Cloner type for `ensure_project_available_locally` (66) and `…_from_facilitator` (150):
`C: Fn(&str, &Path) -> Result<(), String>` — no progress channel.

#### `packages/tddy-worktree-service/src/remote_git_service.rs:616+` — `GitChildRelay`

`tokio::process::Command` with piped stdio and `process_group(0)`; pumps stdout and stderr as separate
chunked frames plus a final exit-code frame. Existing art for streaming a git child's stderr.

### Findings

- Three pre_exec closures call `set_var` (deadlock under std's env lock) and `initgroups` (NSS after
  fork); the set_var is redundant. `tddy_supervisor::spawn_broker::{resolve_target_user, TargetUser}`
  are pub and already a dependency of tddy-spawn.
- `same_user` / `is_this_process` compare **real** ids; the supervisor compares effective ids.
- With set_var removed, a non-root worker gets EPERM from `setgid`; nothing checks privilege up front.
- The worker serializes everything and wedges permanently behind a hung child; concurrent callers can
  swap responses.
- Clone timeouts: forked-worker clones under `spawn_blocking_with_timeout` (CreateProject,
  AddProjectToHost) and `tokio::time::timeout` (session start); supervisor clones under
  `await_supervised_with_timeout`. Worktree: `start_workspace_session` / `start_agent_clone_session`
  `request_timeout`, `create_session_worktree`, `push_new_branch_to_origin_if_requested`.
- CreateProject/AddProjectToHost clone with an **empty env** (no askpass, no agent, no
  `GIT_TERMINAL_PROMPT=0`).
- Test seams: supervisor path takes `git_program` explicitly; worker/in-process path finds `git` via
  `sh` on the child's PATH, so `CloneRequest.env` can carry `PATH=<fake git dir>:…`. Same-user spawns
  skip pre_exec entirely, so a stage pipe is only testable without root if it also runs on the
  same-user path or its steps are data (the supervisor's `pre_exec_plan` precedent).

### Exploration 4: Split start forwarding, keepalive, cancellation — 2026-10-24

**Agent**: Explore subagent
**Scope**: split-start call graph, peer side, kernel/LiveKit forwarding, cancellation, teardown, tests

(The subagent's dump follows verbatim.)

### Discovery dump: streamed split start, clone progress, and cancellation

Repo: /Users/mantasi/.superset/worktrees/tddy-coder/check-logs-under-tried-5db67307 (read-only pass)

#### 1. Sequence (chronological)

1. `ls` of the src dirs of tddy-agent-launch, tddy-session-split, tddy-daemon-kernel and tddy-daemon-livekit, plus `packages/*/tests`. Purpose: map the files.
2. grep across `packages` + `docs` (rs/toml/yaml/md) for `start_forward_deadline|PEER_FORWARD_TIMEOUT|peer_forward_timeout_secs|PEER_FORWARD_STREAM_IDLE_TIMEOUT|forward_start_session_within|forward_stream_start_session|forward_start_naming_local_source|forward_start_session(`. Purpose: find every user of a deadline or forward. (The first try failed on a zsh glob; I re-ran it with quoted `--include`.)
3. Read `packages/tddy-agent-launch/src/start_request_checks.rs` (all of it). Purpose: the unary forward and `provision_project_for_start`.
4. Read `packages/tddy-agent-launch/src/session_coordinate_handlers.rs` (all of it). Purpose: the StreamStartSession and unary StartSession handlers.
5. Read `packages/tddy-agent-launch/src/svc_start_session_core.rs` (all of it). Purpose: routing, placement and the workspace branch.
6. grep `fn start_session_core|start_session_core(`, then `head -150` of `placement.rs`. Purpose: find the real `start_session_core` and the placement classifier.
7. Read `packages/tddy-session-split/src/split_claude_cli_start.rs`. Purpose: where the split forward happens.
8. Read `harness_profile_request.rs` 1-80, and grep `agent_roster.rs` for `split_forward_deadline` and `workspace_start_request`. Purpose: the start_session_core chain, and how the forwarded request is built.
9. Read `split_start.rs` and `svc_paired_codebase_teardown.rs`. Purpose: `SplitStartFailure` and teardown.
10. Read `tddy-daemon-kernel/src/peer_calls.rs` and `peer_forwarding.rs`. Purpose: forward APIs, deadlines and the stream relay.
11. Read `tddy-daemon-kernel/src/stream_relay.rs`, and grep `trait PeerForwarder` in tddy-daemon-extensions. Purpose: idle enforcement and the port contract.
12. Read `tddy-daemon-livekit/src/livekit_peer_forwarder.rs`, and `wc -l` the livekit files. Purpose: the LiveKit stream implementation.
13. `ls` tddy-livekit/src and tddy-rpc/src, and grep `fn call_server_stream|fn call_unary`. Purpose: find the client.
14. grep `cancel|abort|rst|end_of_stream|tx.closed|is_closed` in tddy-livekit/src, tddy-rpc/src and the protos. Then grep `abort` in protos and `.abort|abort:` in rs. Purpose: cancellation semantics.
15. Read `tddy-rpc/proto/rpc_envelope.proto` and `tddy-rpc/src/server_engine.rs`. Read `tddy-livekit/src/client.rs`. Read `tddy-rpc/src/client_engine.rs` 1-320.
16. Read `tddy-session-files/src/attachment_progress.rs` and `tddy-agent-launch/src/start_keepalive.rs`.
17. Read `svc_ensure_project_available_for_start.rs` and `clone_backend.rs`.
18. grep the clone fns in tddy-spawn and `spawn_worker_request_timeout` in config. Read `daemon_methods.rs` 95-115, `spawner.rs` 600-746, `spawn_worker.rs` 215-260 and `supervisor_spawn.rs` 300-386.
19. grep the outlines of `workspace_branch_start.rs` and `workspace_session.rs`, then read `workspace_branch_start.rs` (all) and `workspace_session.rs` 1-170.
20. grep session.proto for the rpc list, `StartSessionEvent`, `StartPhase` and `StartKeepAlive`. Read lines 20-60, and grep the field numbers of the request.
21. grep `begin_phase|end_phase|keep_alive(|with_keepalive` in non-test rs. Grep `progress` in `svc_spawn_split_agent.rs` and `svc_start_sandboxed_codebase_session.rs`, and read 40-140 / 25-140 of each.
22. grep `split_ports.rs` for the struct and trait, and `fn start_workspace_session`. Read `svc_agent_host_ports.rs` 60-110, and grep the callers of the session coordinate handlers.
23. Read `livekit_peer_discovery.rs` 360-400 and 1370-1400.
24. grep `on_peer_disconnected` in non-test rs. grep TS files that use `streamStartSession`. Read `participant/serve.rs` 190-225 and `useSessionAttachments.ts` 365-430.
25. Located the web transport: `tddy-livekit-web/src/transport.ts`, then `tddy-rpc-web/src/envelope-transport.ts` 270-330.
26. grep `cancelstart|cancel_start|CancelSession|CancellationToken` and `rpc Cancel`. Read `tasks.proto` 1-40 and `registry.rs` 30-60, and grep the supervisor client fns.
27. grep tests that touch split, stream start or keepalive. Read `tests/start_forward_deadline.rs`, `tests/start_keepalive.rs` 1-80, and `tests/support/rig.rs` with its pub fns.
28. Outlined and read `tddy-daemon-kernel/tests/common_room_forwarding.rs` 1-135 and 410-460.
29. Outlined four multi-host and phase acceptance suites. Read `remote_managed_worktree_cross_host_acceptance.rs` 378-470 and 900-975. Read `remote_managed_worktree_acceptance.rs` 600-664 and `session_attach_cross_host_acceptance.rs` 668-745.
30. Looked at test helpers: `daemon_rpc_participant/mod.rs`, `support/mod.rs`, the livekit-testkit API, `MockEligibleDaemonSource`, and the `CommonRoom::new(` sites. Read `tddy-session-split/tests/split_room_hosting_port.rs` 96-200 and 390-475, plus the outline of `tddy-daemon-kernel/tests/peer_calls.rs`.
31. Read `agent_roster.rs` 165-190, `svc_provision_agent_clone.rs` 80-120 and `handler_state.rs` 105-125.
32. Read `docs/ft/daemon/remote-managed-worktree.md` 420-475 and the related `docs/dev/todo` entries (four of them, opening lines only).
33. grep web-side timeouts, the cancel flag and phase labels: `useSessionAttachments.ts` 240-250, `CreateSessionPane.tsx`, `CreateSessionStartPhase.tsx`.

#### 2. Inspected files (why, and what in them mattered)

##### packages/tddy-agent-launch/src/session_coordinate_handlers.rs
- `start_session_at_session_coordinate` (l.94-106) is the unary handler. It always passes `AttachmentProgressSink::discarding()`: `self.start_session_core(conn_req, &AttachmentProgressSink::discarding())`. **A forwarded unary StartSession arriving at the peer lands here, so the peer has nowhere to report progress or keepalives.**
- `stream_start_session_at_session_coordinate` (l.294-385):
  - It authenticates first (`resolve_os_user`).
  - `if let PeerRoute::Forward{..} = classify_daemon_route(&req.daemon_instance_id)` calls `peer_calls::forward_stream_start_session(slot,&peer,&req)` and pipes the result through a spawned task into an unbounded channel. This covers **only** the case where `daemon_instance_id` names the peer.
  - Otherwise it builds `AttachmentProgressSink::streaming(conn_progress_tx)` and runs `tokio::spawn(async move { service.start_session_core(req,&sink).await ... let _ = tx.send(event); })`. The task is detached and nothing ties it to whether the client is still connected.
- The relay loops stop forwarding when `tx.send(...).is_err()` (`break`), but the start task keeps running.

##### packages/tddy-agent-launch/src/harness_profile_request.rs (l.28-48)
- The real `start_session_core` is defined here, not in svc_start_session_core.rs. The chain is `start_session_core` → `refuse_a_requested_role` → `apply_harness_profile` → `start_session_with_role` → `start_session_on_request`.
- Its doc comment says it serves StartSession, StreamStartSession, a split's workspace start and project-home.

##### packages/tddy-agent-launch/src/svc_start_session_core.rs — `start_session_on_request`
- l.97-119: `classify_peer_route(local, req.daemon_instance_id, eligible)`. `PeerRoute::Forward` → `forward_start_naming_local_source(&req, os_user, peer)`, which is a **unary forward even when the caller used StreamStartSession**. In practice the stream handler's own forward check runs first, so this unary branch is reached mainly from the unary StartSession path.
- l.125: `classify_start_placement` → `classify_placement(&PlacementRequest{.. requested_codebase_id: req.codebase_daemon_instance_id ..})`.
- l.148-164: `CodebasePlacement::Split{codebase_instance_id}` → `self.split_sessions.start_split_claude_cli_session(os_user, codebase_instance_id, &req, progress)`. **The streaming sink is passed to the split path.** `SandboxedCodebase` → `start_sandboxed_codebase_session(.., progress)`.
- A split skips `provision_project_for_start` locally (l.144-147 comment).
- Peer-side workspace branch (l.217-337):
  - It honours a caller-chosen session id.
  - `prepare_session_attachments(... progress)`.
  - `let timeout = self.config.spawn_worker_request_timeout();`
  - `progress.begin_phase(StartStep::Worktree)` → `seed_and_start_workspace_session(..., timeout)` → `end_phase(Worktree)`.
  - Then the optional SemanticIndex phase, then the sandbox jail, then `announce_worktree_ready`.
- `provision_project_for_start(&req, os_user, progress)` runs at l.171, **before** the workspace branch and outside any phase. No StartPhase covers the clone today.
- l.59-61: `TODO(indexing-indicators)` says split, sandboxed and tool starts report no phases (`docs/dev/todo/2026-10-03-start-phases-...`).

##### packages/tddy-agent-launch/src/placement.rs
- `CodebasePlacement::{CoLocated, Split{codebase_instance_id}, SandboxedCodebase}`.
- `classify_codebase_placement`: empty or self id → CoLocated. A split requires `managed_codebase`, `session_type=="claude-cli"` and the peer in the eligible list. Each refusal comes back as `Err(String)`, which is mapped to InvalidArgument.

##### packages/tddy-agent-launch/src/start_request_checks.rs
- l.79-83 `start_forward_deadline(config)` = `tddy_session_agents::agent_roster::split_forward_deadline(config)`.
- l.99-126 `forward_start_session`:
  - takes `peer_routing.common_room_livekit_room()`, or returns `failed_precondition`;
  - calls `peer_calls::forward_start_session_within(slot,&peer,req, start_forward_deadline(&self.config))`.
- l.130-146 `forward_start_naming_local_source` → `forwarded_start_request(req, local_row)` (it fills `github_source`) → `forward_start_session`.
- l.148-174 `provision_project_for_start(req, os_user, progress)` → `with_keepalive(progress, START_KEEPALIVE_INTERVAL, self.ensure_project_available_for_start(...))`.
- **Correction to the brief:** this unary forward at l.99 is the **whole-session forward** path (`daemon_instance_id` = peer). The **split** forward is a different call site, in `split_claude_cli_start.rs` (below). Both use the same deadline, `split_forward_deadline`.

##### packages/tddy-session-split/src/split_claude_cli_start.rs — `start_split_claude_cli_session` (the split forward)
1. It refuses a recipe and resolves the local specialized agents.
2. `let slot = self.peer_routing.common_room_slot("StartSession")?.clone();`
3. It mints `session_id = Uuid::now_v7()` and `codebase_session_id = Uuid::now_v7()`.
4. `SplitAgentRoom::issued_by(...)`.
5. `workspace_req = agent_roster::workspace_start_request(req, &local_instance_id, &session_id, &codebase_session_id)?`
6. **The unary forward, l.95-101:**
   ```rust
   let forwarded = tddy_daemon_kernel::peer_calls::forward_start_session_within(
       &slot, codebase_instance_id, &workspace_req,
       agent_roster::split_forward_deadline(&self.config),
   ).await;
   ```
7. `Err(status)` → `tear_down_codebase_session(.., &codebase_session_id, .., SplitStartFailure::from_forward_error(&status))`, then return the error.
8. A `branch_conflict` is passed straight back. An empty session id → internal error. A mismatched id → teardown of the reported id with `PeerAnswered`, then an internal error.
9. Then `spawn_split_agent(.., Some(&room), req, progress)`. A spawn error → teardown with `PeerAnswered`.
- **The sink:** the split path *does* receive the caller's real sink (`progress`, streaming when the start came in via StreamStartSession). It is **not used during the forward**: no phase, no keepalive, no relay of peer events. It is used only afterwards, inside `spawn_split_agent` → `attached_initial_prompt(.., progress)` for attachment materialization. So while the peer clones, the local stream to the browser emits **nothing**. The browser→local hop has no relay idle timeout, so the stream itself survives; the 30 s that fails is the hop-2 forward deadline (`split_forward_deadline`).

##### packages/tddy-session-agents/src/agent_roster.rs
- l.39-65 `workspace_start_request`. **These fields mark the forwarded request as the codebase/workspace half:**
  ```rust
  StartSessionRequest {
      session_type: "workspace".to_string(),
      daemon_instance_id: String::new(),
      codebase_daemon_instance_id: String::new(),
      requested_session_id: codebase_session_id.to_string(),
      attachments: Vec::new(),
      specialized_agents, // qualified with agent_instance_id
      split_agent: Some(SplitAgentPlacement { session_id: agent_session_id, agent_daemon_instance_id: agent_instance_id }),
      ..req.clone()
  }
  ```
  Because `..req.clone()` is used, `project_id`, the branch intent fields, `sandbox`, `semantic_index`, `session_token`, `managed_codebase`, `github_source` and the rest are carried over.
- l.171-188 `split_forward_deadline(config) = config.spawn_worker_request_timeout() + config.peer_forward_timeout()`. The default is 300 + 30 = 330 s.

##### packages/tddy-session-split/src/split_start.rs
- `SplitStartFailure::{PeerAnswered, ForwardDeadline}`. `from_forward_error`: only `Code::DeadlineExceeded` → `ForwardDeadline`; every other code → `PeerAnswered`.
- `resolve_split_agent_placement`: `split_agent` is allowed only on `workspace`, never with `agent_clone`, and both of its fields are required.
- `resolve_caller_chosen_session_id`: workspace only, validated as a path segment.

##### packages/tddy-session-split/src/svc_paired_codebase_teardown.rs
- `tear_down_codebase_session(slot, codebase_instance_id, codebase_session_id, token, unwinding)` sends `peer_calls::forward_delete_session` (unary, ordinary 30 s deadline). Outcomes:
  - Ok → info.
  - `peer_has_no_such_session(&e)` with `PeerAnswered` → info.
  - `peer_has_no_such_session(&e)` with `ForwardDeadline` → **warn**: "may create the session after this teardown, leaving an orphaned checkout".
  - Any other error → error, "worktree is now orphaned".
- `delete_paired_codebase_session` is used by normal DeleteSession. It checks `slot.is_connected()` first and is idempotent on "no such session".

##### packages/tddy-session-split/src/svc_spawn_split_agent.rs (l.55-140)
- `spawn_split_agent(.., room, req, progress)`: creates the session dir, `attached_initial_prompt(.., progress)`, wires the tools, `host_split_room`, `split_agent_roster` (a forward to the codebase host), `split_agent_context_and_args` (context fetch over the peer link), then spawns. No `begin_phase(Agent)`.

##### packages/tddy-session-split/src/svc_start_sandboxed_codebase_session.rs (l.25-140)
- This is the same-host analogue. It calls `self.host.start_workspace_session(workspace_req, &AttachmentProgressSink::discarding())`, so the workspace half's progress is discarded explicitly. It sets `sandbox: true, sandboxed_codebase: false`. On failure it tears down with `tear_down_local_checkout_session`.

##### packages/tddy-session-split/src/split_ports.rs
- `pub trait SplitHost: AgentHostCallbacks { async fn start_workspace_session(req, progress); async fn delete_session(..); ...}` (l.60-71).
- `pub struct SplitSessions { config, tddy_data_dir, staging_base_dir, peer_routing, split_rooms, reachable_sessions, workspace_sandboxes, workspace_sandbox_provisioner, claude_cli_manager, session_tokens, agent_roster, host: Arc<dyn SplitHost> }` (l.89-108).
- The impl is in `tddy-session-lifecycle/src/connection_service/svc_agent_host_ports.rs` l.80-88: `start_workspace_session` → `launch_sessions().start_session_core(req, progress)`.

##### packages/tddy-agent-launch/src/workspace_branch_start.rs
- `seed_and_start_workspace_session(req, os_user, paired_agent, sessions_base, session_id, timeout)`: `seeded_roster_records` → `workspace_session::start_workspace_session(..., &self.tddy_data_dir, timeout)` → read `SeedCodebase` → `seed_session_agent_roster`.

##### packages/tddy-session-split/src/workspace_session.rs — `start_workspace_session` (l.54-167)
1. `find_registered_project` and `project_repo_root`. The clone must already have happened in `provision_project_for_start`.
2. `resolve_branch_workflow`.
3. **It creates the session dir and writes the changeset BEFORE the worktree is cut.** Then:
   ```rust
   let worktree_path = tokio::time::timeout(request_timeout, tokio::task::spawn_blocking(move || {
       tddy_core::setup_worktree_for_session_with_optional_chain_base(&repo_root_clone,&session_dir_clone, base_ref.as_deref()) ...
   })).await.map_err(|_| Status::deadline_exceeded("start_workspace_session: create worktree timed out"))?
   ```
   This is the **worktree-creation `request_timeout`** the plan wants to remove. Its value is `spawn_worker_request_timeout` (300 s by default). If it times out, the blocking git work keeps running, and the session dir is left behind with no `.session.yaml`.
4. Writes `.session.yaml` with `agent_daemon_instance_id` / `agent_session_id`.
- `start_agent_clone_session` (l.183+) has the same `tokio::time::timeout(request_timeout, spawn_blocking(..))` pattern.

##### packages/tddy-agent-launch/src/svc_ensure_project_available_for_start.rs
- `ensure_project_available_for_start`:
  - `let timeout = self.config.spawn_worker_request_timeout();`
  - resolves the facilitating daemon for an agent clone;
  - fans out `peer_project_entries` if the project is not registered locally;
  - `spawn_project_clone(ProjectClone{..})` runs `tokio::task::spawn_blocking`, calling `ensure_project_available_from_facilitator` or `ensure_project_available_locally` with a `cloner` closure → `CloneBackend::clone_repo(git_url, dest, planned.env())`.
  - `match tokio::time::timeout(timeout, handle).await { ... Err(_elapsed) => Err(Status::deadline_exceeded("ensure_project_available_for_start: clone timed out")) }` is **another timeout on the clone**, also `spawn_worker_request_timeout`.
  - Nothing reports progress, and nothing cancels the work: the blocking task is simply abandoned.

##### packages/tddy-agent-launch/src/clone_backend.rs
- `CloneBackend::clone_repo` picks one of three paths:
  - `Supervisor{socket_path}` → `runtime.block_on(clone_repo_via_supervisor_with_env(..))`;
  - `ForkedWorker` with a client → `client.clone_repo(CloneRequest{..})`, which is a synchronous pipe request/response;
  - no worker → `spawner::clone_as_user_with_env`.

##### packages/tddy-spawn (clone implementations)
- `spawner.rs:624` `clone_as_user_with_env`: `sh -c 'if test -e "$1"; then exit 0; fi; exec git clone "$2" "$1"'`, with stdout/stderr piped and `cmd.output()`. It **blocks until exit**, does not pass `--progress`, streams no output, and has no kill handle.
- `spawn_worker.rs:225` `SpawnClient::clone_repo` → `send_and_recv` holds the request/response mutexes and does a blocking `read_line`. This **serializes all worker requests**, and nothing can cancel one.
- `supervisor_spawn.rs:317` `clone_repo_via_supervisor_with_env`: `client.spawn_session(clone_request_with_env(..))` returns a pid, then `wait_for_exit` polls `session_status(pid)` every 25 ms. The supervisor client also has `stop_session(pid)` (`tddy-supervisor/src/client.rs:170`), **so a kill is possible on the supervisor backend**.
- `config/daemon_methods.rs:104`: `spawn_worker_request_timeout()` = `spawn_worker_request_timeout_secs.max(1)`, default 300 (`config.rs:17,114`).

##### packages/tddy-agent-launch/src/start_keepalive.rs
- `START_KEEPALIVE_INTERVAL = 5s` and `PROVISIONING_ACTIVITY = "provisioning"`.
- `with_keepalive(sink, interval, work)` is a biased `select!` of the work against `interval_at(now+interval)` ticks calling `sink.keep_alive(...)`. On a discarding sink it does nothing.
- It is used **only** in `provision_project_for_start`.

##### packages/tddy-session-files/src/attachment_progress.rs
- `pub struct AttachmentProgressSink { pub tx: Option<UnboundedSender<Result<StartSessionEvent, Status>>> }`.
- `discarding()` sets `tx: None`; `streaming(tx)` wraps a sender.
- `report`, `begin_phase`, `end_phase` and `keep_alive` all do `let _ = tx.send(...)`. The doc says: "A closed receiver (the client hung up) is ignored: the session start is already under way and is not abandoned because nobody is watching." **There is no cancellation signal, and a failed send is deliberately swallowed.**

##### packages/tddy-service/proto/session.proto
- `rpc StartSession`, `rpc StreamStartSession(StartSessionRequest) returns (stream StartSessionEvent)`, `SignalSession`, `DeleteSession` (l.24-57). There is no cancel RPC.
- `StartSessionEvent { oneof event { AttachmentMaterializationProgress attachment_progress=1; StartSessionResponse result=2; StartPhase phase=3; StartKeepAlive keep_alive=4; } }` (l.432-439).
- `StartPhase.Step { UNSPECIFIED=0, STEP_WORKTREE=1, STEP_SEMANTIC_INDEX=2, STEP_AGENT=3 }`, `Boundary{BEGIN=1, END=2}` (l.104-121). **There is no clone step**, and STEP_WORKTREE's comment says "Fetching the project and cutting the session's git worktree."
- `StartKeepAlive { string activity = 1; }`.
- Request fields: `daemon_instance_id=5`, `session_type=7`, `managed_codebase=17`, `codebase_daemon_instance_id=32`, `requested_session_id=33`, `agent_clone=34`, `split_agent=35`.

##### packages/tddy-daemon-kernel/src/peer_calls.rs
- `forward_start_session(room,peer,req)` uses `room.forward_timeout()`.
- `forward_start_session_within(room,peer,req,deadline)` → `room.forward_to_peer_within(peer,"session.SessionService","StartSession",body,deadline)`.
- `forward_stream_start_session(room,peer,req)` → `room.forward_server_stream_to_peer(peer,"session.SessionService","StreamStartSession",body, decode StartSessionEvent)`. **It has no explicit-deadline variant.**
- `forward_delete_session` is unary with the room deadline.

##### packages/tddy-daemon-kernel/src/peer_forwarding.rs
- `PEER_FORWARD_TIMEOUT = 30s` (l.104) is the default for unary calls and for stream *open*.
- `PEER_FORWARD_STREAM_IDLE_TIMEOUT = 30s` (l.117) is fixed. Its lower bound is tied to `PASS_LONG_ENOUGH_TO_BE_SERVICE` and pinned by `tddy-daemon-rpc/tests/session_agent_roster_acceptance.rs:600`.
- `CommonRoom { forwarder: Arc<dyn PeerForwarder>, forward_timeout }`, built by `CommonRoom::new(forwarder, &config)` with `config.peer_forward_timeout()`.
- `forward_to_peer_within` = `tokio::time::timeout(deadline, forwarder.call_unary(..))`. On expiry → `peer_forward_deadline_status`, which is DEADLINE_EXCEEDED: "forwarding {svc}/{m} to daemon {peer} timed out after {n}s: the peer is in the common room but its RPC participant did not answer".
- `forward_server_stream_to_peer`:
  - `open_deadline = self.forward_timeout`;
  - `timeout(open_deadline, forwarder.call_server_stream(..))`;
  - then spawns `stream_relay::Relay{ idle_timeout: PEER_FORWARD_STREAM_IDLE_TIMEOUT, .. }.run(frames, tx)` into an **unbounded** channel.

##### packages/tddy-daemon-kernel/src/stream_relay.rs
- `Relay::run` loops `timeout(idle_timeout, frames.recv())`. Every frame restarts the timer, keepalives included. On idle expiry it sends one `idle_status()` error and stops. An error frame is terminal. `Ok(None)` is a clean end.
- It notices a consumer drop only when it next tries `tx.send` (a frame or the idle expiry).

##### packages/tddy-daemon-extensions/src/peer_forwarder.rs
- `trait PeerForwarder { is_connected; call_unary(peer,svc,m,body) -> Result<Vec<u8>>; call_server_stream(..) -> Result<mpsc::Receiver<Result<Vec<u8>,Status>>> }`. The doc says "No deadline: the caller applies it."
- `NoPeerForwarding` returns `failed_precondition` for everything.

##### packages/tddy-daemon-livekit/src/livekit_peer_forwarder.rs
- `LiveKitPeerForwarder{slot: CommonRoomSlot}`. `peer_client` → `LiveKitRpcClientFactory::for_room(room).client(daemon_rpc_identity(peer))`, sharing one engine and one response loop per room.
- `call_server_stream` → `client.call_server_stream(..)`, then spawns a relay with `STREAM_RELAY_DEPTH=32` that `select!`s `frames.recv()` against `tx.closed()`. When the reader goes away it stops and drops the client. **It does not notify the peer.**

##### packages/tddy-daemon-livekit/src/livekit_peer_discovery.rs
- l.372-375 re-exports `classify_peer_route, daemon_rpc_identity, CommonRoom, PeerRoute, PEER_FORWARD_STREAM_IDLE_TIMEOUT, PEER_FORWARD_TIMEOUT` from the kernel.
- l.1382-1396 re-exports the `peer_calls::*` functions under their old names (`forward_start_session_within as forward_start_session_via_livekit_within`, `forward_stream_start_session as forward_stream_start_session_via_livekit`, and so on).
- It contains **no stream-forwarding implementation of its own**. The relay is the kernel `CommonRoom` + `stream_relay`, plus the transport relay in `LiveKitPeerForwarder`.

##### packages/tddy-livekit/src/client.rs
- `call_server_stream`: `engine.begin_stream(..)`, `publish_request(request).await?`, `Ok(rx)`. **It returns as soon as the request is published, without waiting for a first frame.** So the stream "open deadline" only bounds publishing; in practice the 30 s idle timeout covers the time to the first frame.
- `call_unary` publishes, then awaits the oneshot.
- Nothing is published when the receiver is dropped.

##### packages/tddy-rpc/src/client_engine.rs
- `register_stream` uses `mpsc::channel(32)`. `build_request` and `build_continuation` always set `abort: false`.
- `on_response`: epoch and call-metadata checks, then `tx.send(result).await` with backpressure.
- A dropped receiver leaves its pending entry in place until a terminal frame arrives (the send fails silently). There is no deregistration and no cancel frame.

##### packages/tddy-rpc/proto/rpc_envelope.proto
- `RpcRequest { int32 request_id=1; bytes request_message=2; CallMetadata call_metadata=3; Metadata metadata=4; bool end_of_stream=5; bool abort=6; optional string sender_identity=7; uint32 client_epoch=8; }`.
- **The `abort` field exists on the wire, but nothing sets it to true and no server reads it.** My grep found only `abort: false` literals in src and tests.

##### packages/tddy-rpc/src/server_engine.rs
- `on_request` routes bidi and multi-message continuations, then calls `spawn_dispatch` for unary and server-stream calls. **It never inspects `request.abort`.**
- `spawn_dispatch` → `PeerForwards::spawn(peer, async { bridge.handle_messages(..); forward_response_body(..) })`.
- `forward_response_body` with `Streaming(rx)` forwards each item onto the shared `outgoing` channel. It stops only if `outgoing.send` fails (the whole transport is down) or on an error item.
- `on_peer_disconnected(peer)` drops bidi and multi-message state and runs `peer_forwards.abort_all(peer)`, which aborts the JoinHandles and awaits them. This is the **only server-side cancellation**, and it is per participant (e.g. a participant leaving the LiveKit room), not per call. Aborting a future does not stop a `spawn_blocking` closure.

##### packages/tddy-livekit/src/participant/serve.rs l.203-212
- `RoomEvent::ParticipantDisconnected(remote)` → `self.server.on_peer_disconnected(identity)`.
- Other `on_peer_disconnected` callers are in tddy-tauri-rpc (`host.rs`, `multi_host.rs`).

##### Web side
- `packages/tddy-web/src/hooks/useSessionAttachments.ts` l.377-418 `startSessionStreamed`: a comment says "Consumed with a `cancelled` flag rather than an `AbortSignal`: the LiveKit transport accepts a signal for server-streaming calls and never reads it". It handles the `attachmentProgress`, `result` and `phase` cases. `keepAlive` is implicitly ignored.
- `cancelledRef` is set only on unmount (l.240-248).
- `packages/tddy-rpc-web/src/envelope-transport.ts` l.291-298:
  ```ts
  // TODO: cancellation is local only — the peer is not told, so it keeps serving a stream nobody
  // reads until it ends on its own. `RpcRequest.abort` is the field for saying so.
  const onAbort = () => { this.options.log?.(`cancel request_id=...`); this.calls.cancelCall(requestId); };
  ```
  A `timeoutMs` deadline is optional, and a caller that passes none waits indefinitely.
- `packages/tddy-web/src/components/sessions/CreateSessionStartPhase.tsx` l.5-8: phase labels `WORKTREE: "Creating worktree…"`, `SEMANTIC_INDEX`, `AGENT`. There is no clone label.
- `CreateSessionPane.tsx` has an `onCancel` prop (closes the pane) and a branch-conflict cancel. **There is no start-cancel action.**

##### Other forward_start_session_within caller
- `packages/tddy-session-agents/src/svc_provision_agent_clone.rs` l.95-101: an agent-clone workspace start on a peer, using `split_forward_deadline` with the same unary pattern. On an id mismatch → `delete_clone_on_peer`.
- `tddy-session-lifecycle/src/connection_service/handler_state.rs:114` `split_forward_deadline(&self)` is a test-facing accessor used by `remote_managed_worktree_acceptance.rs`.

##### Task registry (an existing cancel primitive)
- `packages/tddy-service/proto/tasks.proto`: `TaskService.CancelTask` (cooperative cancel via `CancellationToken`; SIGINT, then SIGKILL after about 5 s), `WatchTask`. Remote forwarding is deferred: it returns FAILED_PRECONDITION for a remote `daemon_instance_id`.
- `packages/tddy-task/src/registry.rs`: `TaskRegistry` with per-task PIDs and `CancellationToken`.

##### Docs and todos
- `docs/ft/daemon/remote-managed-worktree.md` l.428-449 ("Failure is atomic"): the caller-chosen id plus a forward that outlives the peer's budget (`spawn_worker_request_timeout + peer_forward_timeout_secs`, about 330 s). The text says it "is the price of never orphaning a checkout". l.469-475: long-running tools must use background jobs; it explicitly says "not a new keepalive mechanism".
- `docs/dev/todo/2026-09-15-rpcservice-has-no-cancellation-surface.md`:
  - `RpcService` gets no cancellation token.
  - Abort exists only at transport level (`on_peer_disconnected`).
  - The tonic/UDS and connectrpc paths have no abort at all.
  - `spawn_blocking` work runs to completion anyway.
  - The warm-index workaround uses `TaskRegistry` plus "a disconnect detected by a send failing into the response stream's dropped receiver".
- `docs/dev/todo/2026-10-03-start-phases-...-split-starts.md`: split starts report no phases.
- `docs/dev/todo/2026-10-17-a-timed-out-clone-leaves-a-partial-destination-that-later-starts-skip.md`: an abandoned clone child keeps running, and a partial `<repos>/<name>` makes later starts skip the clone. `Command::output()` cannot be killed.
- `packages/tddy-agent-launch/docs/module-layout.md:43` and `docs/changesets/2026-10-17-clone-ssh-passphrase.md:5` describe `start_forward_deadline` and the 5 s keepalive.
- `docs/ft/daemon/livekit-peer-discovery.md:17-20` and `packages/tddy-daemon-kernel/docs/daemon-kernel.md:239-289` define the config key `peer_forward_timeout_secs`.

#### 3. Grep / glob results

- **Deadline symbols** (`packages`, `docs`):
  - `start_forward_deadline`: defined at `start_request_checks.rs:79`, exported at `lib.rs:95`, used at `start_request_checks.rs:117`, tested in `tests/start_forward_deadline.rs:14,25,48`.
  - `split_forward_deadline`: defined at `agent_roster.rs:186`; used at `split_claude_cli_start.rs:99`, `svc_provision_agent_clone.rs:99` and `handler_state.rs:114`.
  - `forward_start_session_within` call sites: `split_claude_cli_start.rs:95`, `svc_provision_agent_clone.rs:95`, `start_request_checks.rs:113`.
  - `forward_stream_start_session` call site: `session_coordinate_handlers.rs:319` (the only one).
- **`peer_forward_timeout_secs` users:**
  - definition and default: `config.rs:21-22,119-120,255`; tests at `config.rs:1567,1574`;
  - accessor: `config/daemon_methods.rs:109-111`;
  - read by `CommonRoom::new` (`peer_forwarding.rs:134`) and by `split_forward_deadline`;
  - doc mention at `tddy-host-service/src/peer_routing.rs:67`;
  - test configs: `tddy-daemon-kernel/tests/common_room_forwarding.rs:117`, `tddy-daemon/tests/session_attach_cross_host_acceptance.rs:162-309`, `tddy-session-lifecycle/tests/remote_managed_worktree_acceptance.rs:93`.
- **`PEER_FORWARD_STREAM_IDLE_TIMEOUT` users:**
  - `peer_forwarding.rs:117,236,243`; `start_keepalive.rs:4`;
  - `tddy-session-lifecycle/src/connection_service.rs:55,68` (a const assert that some interval is below it);
  - `tddy-session-files/src/svc_materialize_staged_attachment.rs:132`, `attachment_progress.rs:114`;
  - `tddy-tools/tests/session_tool_livekit_dispatch.rs:284,291`, `tddy-daemon-rpc/tests/session_agent_roster_acceptance.rs:600`, `tddy-daemon-kernel/tests/common_room_forwarding.rs:13,312`.
- **`abort`** in protos: only `tddy-rpc/proto/rpc_envelope.proto:13`. In rs: only `abort: false` literals (`client_engine.rs:165,185`, `tddy-livekit/src/envelope.rs:28,58`, `participant.rs:440`, plus tests) and JoinHandle `.abort()` calls.
- **Cancel APIs** (`cancelstart|CancelSession|CancellationToken|rpc Cancel`): no start-cancel RPC. Found `tasks.proto:22 CancelTask`, `session_agents.proto:62 CancelAgentConversation`, and `code_navigation.proto:126 CancelCodeQuery`. CancellationToken is used in tddy-task and tddy-index-daemon.
- **Phase and keepalive emitters** (non-test): `svc_start_session_core.rs:264,275,283,298`, `claude_cli_spawn.rs:149,165,247,267`, `cursor_cli_spawn.rs:263,274`, `opencode_cli_spawn.rs:204,222`, `cli_session_worktree.rs:80,82,189,198`, `claude_cli_spawn_steps.rs:259,268`, `svc_start_sandboxed_cli_harness_session.rs:264,306`. `with_keepalive` appears only at `start_request_checks.rs:160`. **tddy-session-split has none.**
- **`on_peer_disconnected` callers:** `tddy-livekit/src/participant/serve.rs:211`, `tddy-tauri-rpc/src/host.rs:182,241`, `multi_host.rs:255,319`.
- **TS `streamStartSession` users:** `tddy-web/src/hooks/useSessionAttachments.ts`, plus Cypress components (`CreateSessionClonePassphraseMount.cy.tsx`, `SessionStartAndIndexingProgress.cy.tsx`, `CreateSessionAttachmentProgress.cy.tsx`, `ServerStreamFallback.cy.tsx`, and others) and `cypress/support/rpc/connectionRpcs.ts`.
- **Tests touching split, stream start or keepalive:** see §4.6.

#### 4. Findings

##### 4.1 Call graph of a split start (today)

**Browser → local daemon A** (`session.SessionService/StreamStartSession`, over LiveKit `daemon-{A}` or HTTP):

1. `LaunchSessions::stream_start_session_at_session_coordinate` (session_coordinate_handlers.rs:294)
   - `resolve_os_user`
   - `classify_daemon_route(req.daemon_instance_id)` → Local, because the agent runs on A
   - `AttachmentProgressSink::streaming(tx)`, then `tokio::spawn(start_session_core(req,&sink))`, detached
2. `start_session_core` (harness_profile_request.rs:28) → `start_session_with_role` → `start_session_on_request` (svc_start_session_core.rs:62)
   - `classify_peer_route` → Local
   - `classify_start_placement` → `placement::classify_placement` → `classify_codebase_placement` → `Split{codebase_instance_id: B}`
   - `self.split_sessions.start_split_claude_cli_session(os_user, B, &req, progress)`
3. `split_claude_cli_start.rs:30`
   - `resolve_specialized_agent_defs`
   - `common_room_slot`
   - mint `session_id` and `codebase_session_id`
   - `SplitAgentRoom::issued_by`
   - `agent_roster::workspace_start_request(..)`
   - **`peer_calls::forward_start_session_within(&slot, B, &workspace_req, split_forward_deadline)`**: unary, deadline 330 s by default (`spawn_worker_request_timeout_secs` + `peer_forward_timeout_secs`)
4. `CommonRoom::forward_to_peer_within` → `tokio::time::timeout(deadline, LiveKitPeerForwarder::call_unary)` → `RpcClient::call_unary` publishes to `daemon-{B}` and awaits a oneshot.
5. On error: `tear_down_codebase_session(.., SplitStartFailure::from_forward_error)` → `forward_delete_session` (30 s).
6. On OK: id checks, then `spawn_split_agent(.., progress)` (svc_spawn_split_agent.rs:55), which covers attachments (progress used), tools wiring, `host_split_room`, `split_agent_roster` (a forward), context fetch (a forward), and the agent spawn. On failure → teardown with `PeerAnswered`.

**Sink:** the split path holds the real streaming sink. It is not discarding, but it is **unused during the forward**: no phase, no keepalive, no relaying of B's events. The workspace half of the **same-host** sandboxed-codebase placement does use `AttachmentProgressSink::discarding()` explicitly (svc_start_sandboxed_codebase_session.rs). On B, the forwarded unary call always runs with `discarding()` (session_coordinate_handlers.rs:101).

**The incident's "30 s":** the brief says the forward had a 30 s deadline. On default config, `split_forward_deadline` is 330 s. A 30 s failure points to one of:
- a non-default `spawn_worker_request_timeout_secs`;
- the whole-session forward (`start_request_checks.rs:99`, whose deadline is also `split_forward_deadline`);
- or a different leg using `PEER_FORWARD_TIMEOUT`, such as a stream open, the stream idle timeout, or the 30 s teardown and roster forwards.

The logs should confirm which deadline message appeared ("timed out after {n}s").

**Whole-session forward path** (`daemon_instance_id` = peer, not split): the stream handler calls `forward_stream_start_session` (already streaming, open deadline 30 s, idle 30 s). The unary handler calls `forward_start_naming_local_source` → `forward_start_session(... start_forward_deadline)`.

**Fields that mark the forwarded request as the workspace half:**
- `session_type="workspace"`
- `split_agent=Some{session_id: A-side agent session, agent_daemon_instance_id: A}`
- `requested_session_id=codebase_session_id`
- `daemon_instance_id=""` and `codebase_daemon_instance_id=""`, so B routes Local / CoLocated
- `attachments=[]`
- `specialized_agents` qualified with A's id
- everything else copied from the original request

##### 4.2 Peer (B) side today
1. The LiveKit participant `daemon-{B}` → `ServerEngine::on_request` → `spawn_dispatch` (attributed to peer `daemon-{A}`'s RPC identity) → `start_session_at_session_coordinate` → `start_session_core(req, &discarding())` → `start_session_on_request`.
2. Route Local → placement CoLocated → `resolve_caller_chosen_session_id` → `resolve_split_agent_placement` → `PairedAgentSession`.
3. **`provision_project_for_start(&req, os_user, progress)`** (svc_start_session_core.rs:171) → `with_keepalive(discarding, 5s, ensure_project_available_for_start(..))`. Keepalives go nowhere, and the clone runs under `tokio::time::timeout(spawn_worker_request_timeout, spawn_blocking(clone))`.
4. Clone backend: supervisor (pid, pollable, `stop_session` exists), forked worker (blocking pipe, serialized, uncancellable), or in-process `Command::output()` (uncancellable, no progress, not run with `--progress`).
5. Stack-seed validation and the branch-conflict check.
6. Workspace branch: `prepare_session_attachments` (empty) → **`progress.begin_phase(StartStep::Worktree)`** → `seed_and_start_workspace_session` → `workspace_session::start_workspace_session(.., request_timeout = spawn_worker_request_timeout)`. That function creates the session dir and changeset, then runs `timeout(request_timeout, spawn_blocking(setup_worktree_for_session_with_optional_chain_base))`, then writes `.session.yaml`. Then `seed_session_agent_roster` → `end_phase(Worktree)`.
7. Then the optional `begin/end_phase(SemanticIndex)` around `index_workspace_worktree`, then `provision_workspace_tool_sandbox` if `sandbox`, then `announce_worktree_ready`.

Phase emitters on this path are `svc_start_session_core.rs:264/275` (Worktree) and `283/298` (SemanticIndex). The keepalive emitter is `start_request_checks.rs:160` (provisioning only). The sink is whatever `start_session_core` was given, which on B for a forwarded unary call is `discarding()`.

Nothing emits a keepalive during the worktree cut, the roster seed, the index or the jail. On a streamed hop, any of those over 30 s would trip the idle timeout. The worktree cut runs `git fetch` + `worktree add`.

##### 4.3 Kernel and LiveKit forwarding
- `forward_start_session_within`: a hard overall deadline via `tokio::time::timeout` around `call_unary`.
- `forward_stream_start_session` → `forward_server_stream_to_peer`. The **open deadline** is `room.forward_timeout()` (`peer_forward_timeout_secs`, default 30), but the LiveKit `call_server_stream` returns once the request is published, so the open deadline does not wait for the first frame. The **idle timeout** is the fixed `PEER_FORWARD_STREAM_IDLE_TIMEOUT` (30 s) per frame (`stream_relay::Relay`), and any frame, keepalive included, resets it. The relay channel is unbounded.
- `PEER_FORWARD_TIMEOUT` (30 s) exists only as the default of `peer_forward_timeout_secs`.
- `start_forward_deadline(config)` = `split_forward_deadline(config)` = `spawn_worker_request_timeout()` + `peer_forward_timeout()`. It has **no config key of its own**: it derives from `spawn_worker_request_timeout_secs` (default 300) and `peer_forward_timeout_secs` (default 30).
- LiveKit relay layers:
  1. `LiveKitPeerForwarder::call_server_stream` relays with `mpsc(32)`, `select!`s on `tx.closed()`, and drops the client when the reader goes;
  2. `RpcClient` publishes through the shared `ClientEngine` with `channel(32)`;
  3. the room's shared response loop.
- `livekit_peer_discovery.rs` only re-exports.

##### 4.4 Cancellation
- **Client drops a server stream (Rust).** The `ClientEngine` pending stream stays registered until a terminal frame arrives. No frame is sent to the server: `abort` is always false. The server's `forward_response_body` keeps pushing to the shared `outgoing` channel; frames reach the client, which drops them.
- **The server handler is not notified.** In `stream_start_session_at_session_coordinate` the start runs on a detached `tokio::spawn`. The `AttachmentProgressSink` ignores send errors by design ("not abandoned because nobody is watching"). The only observable signal is `tx.send(..).is_err()` in the relay tasks, which just `break` the relay.
- **The only server-side cancel is per participant:** `ServerEngine::on_peer_disconnected` (LiveKit `ParticipantDisconnected`; Tauri) aborts all of that peer's in-flight dispatch tasks at their next await point. `spawn_blocking` clones and worktree cuts continue regardless. The tonic/UDS and connectrpc paths have no abort at all (todo 2026-09-15).
- **Across a forwarded stream:** when A's consumer drops, the kernel relay notices on its next send or idle tick and drops the transport receiver. `LiveKitPeerForwarder`'s relay sees `tx.closed()` and drops the `RpcClient`. B is **not** told and keeps running the start, and later frames are discarded by A's ClientEngine. There is no cancel propagation today.
- **Browser:** `tddy-rpc-web` envelope-transport's AbortSignal is local only, with an explicit TODO naming `RpcRequest.abort`. `useSessionAttachments` deliberately uses a `cancelled` flag instead. There is no Cancel button for a start in progress.
- **No RPC cancels an in-flight start.** Closest primitives:
  - `tasks.TaskService.CancelTask` (local-only, and remote is FAILED_PRECONDITION);
  - supervisor `stop_session(pid)` for supervisor-backed clones;
  - `DeleteSession` (teardown after the fact);
  - the `RpcRequest.abort` wire field, which exists but is not implemented on any side.

##### 4.5 Teardown and orphan logic today, and what changes without a deadline
- `ForwardDeadline` (DEADLINE_EXCEEDED): the teardown is blind. If B answers "no such session", A warns about a possible orphan, because B may still be building and create the session after the delete.
- `PeerAnswered` (any other error, an id mismatch, or a failed local agent spawn): "no such session" is final.
- B leaves a partial session dir when the worktree cut fails. The cross-host test `a_worktree_failure_on_the_codebase_daemon_leaves_no_session_behind` depends on A's teardown to remove it.
- With no overall deadline and a streamed forward, the outcomes become:
  - the stream ends with a result → OK;
  - the stream ends with a peer-sent error → `PeerAnswered` semantics;
  - the stream ends with the relay idle error (DEADLINE_EXCEEDED from `peer_forward_deadline_status`) → still ambiguous. Today `from_forward_error` maps it to `ForwardDeadline`, so the orphan warning remains possible unless B also tears down its own partial work when its upstream disappears;
  - a user cancel → needs a new propagation path, either an `abort` frame or a cancel RPC to B, and B-side self-cleanup.
- The `remote-managed-worktree.md` "Failure is atomic" rationale (the about-330 s split deadline) and the tests pinning `split_forward_deadline` would need revising (see §4.6).
- `ensure_project_available_for_start`'s `timeout(spawn_worker_request_timeout)` and `start_workspace_session`'s `request_timeout` both abandon blocking work without killing it (todo 2026-10-17 on partial clone destinations).
- `svc_provision_agent_clone.rs` is a second unary user of `split_forward_deadline`, for agent-clone workspace starts.

##### 4.6 Existing tests and reusable helpers
- `packages/tddy-agent-launch/tests/start_forward_deadline.rs`: pins `start_forward_deadline > PROMPT_TTL`, `!= 30s`, and `== split_forward_deadline`. **It will need rewriting** if the deadline goes away.
- `packages/tddy-agent-launch/tests/start_keepalive.rs`: paused-clock unit tests of `with_keepalive`. Helpers `a_streaming_sink()`, `drained(rx)`, `takes(dur)`.
- `packages/tddy-agent-launch/tests/support/` (`rig.rs`, `sshd.rs`, `ssh_agent.rs`, `operator.rs`): a real git/ssh/sshd/askpass clone rig — `a_clone_rig(sshd, answer)`, `an_sshd().authorizing_a_passphrase_protected_key(..).started()`, `CloneRig::clone_the_repository`, `ScriptedOperator`, `a_running_ssh_agent`, `A_CLONE_THAT_HANGS = 60s`. Plus `clone_askpass_acceptance.rs` and `clone_ssh_env.rs`.
- `packages/tddy-daemon-kernel/tests/common_room_forwarding.rs`: **an in-memory `AScriptedPeer: PeerForwarder`** with `Answer::{Bytes, Failure, Never, Frames(mpsc::Receiver), OpenFailure}`. Helpers `a_room_over`, `a_room_streaming(frames)`, `relayed_as_text`, `a_config_with_forward_timeout`. Tests include `a_stream_that_goes_quiet_ends_with_one_deadline_error` and `a_consumer_that_drops_the_relay_releases_the_forwarders_stream` (paused clock).
- `packages/tddy-daemon-kernel/tests/peer_calls.rs`: `OnePeerCall` scripted forwarder, `a_room_over`.
- `packages/tddy-daemon-kernel/src/stream_relay.rs` unit tests: frames every 20 s never time out, silence errors, keep-alive bytes pass through.
- `packages/tddy-session-split/tests/split_room_hosting_port.rs`: **the best in-memory split fixture.**
  - `a_facilitating_daemon()` builds a real `SplitSessions` over `PeerRouting::new(config, LocalOnlyEligibleDaemonSource, Some(CommonRoom::new(ACodebaseHost)))`.
  - `ACodebaseHost: PeerForwarder` scripts the unary `StartSession` and `DeleteSession`. Its `call_server_stream` panics today, so that would be the place to script streamed `StartSessionEvent` frames.
  - Also `RecordedSplitRooms`, `TheRestOfTheHost` (a `SplitHost` stub whose every method panics), `an_agent_roster_with_nothing_attached`, `a_split_claude_cli_request`, `a_toolchain_holding_tddy_tools`, and `CreatedCheckout` / `the_checkout_it_created()`.
- `packages/tddy-session-lifecycle/tests/remote_managed_worktree_acceptance.rs`: no LiveKit. `MockEligibleDaemonSource`, `service_with_known_codebase_peer[_and_config]`, `config_allowing_a_worktree_to_take_secs`, `a_split_claude_cli_request`. Tests `the_split_forward_waits_out_the_worktree_budget_plus_one_ordinary_forward` and `the_split_forward_outlives_the_worktree_budget_the_codebase_daemon_gives_itself` pin the deadline via `TestDaemon::split_forward_deadline()`. **Both need rewriting.**
- `packages/tddy-session-lifecycle/tests/sandboxed_codebase_placement_acceptance.rs`: has its own `MockEligibleDaemonSource`.
- `packages/tddy-session-lifecycle/tests/start_phase_acceptance.rs`: in-process claude-cli start with the phases asserted. Helpers `a_daemon_with_a_project`, `a_session_host(config, sessions_base)`, `a_repository_whose_origin_is_itself`, `register_project`, `everything_the_start_said(stream)`, `what_it_says(event) -> Said`, `next_item`.
- `packages/tddy-session-lifecycle/tests/workspace_session_start_acceptance.rs`, `workspace_session_acceptance.rs`: exist (names only; I did not open them).
- `packages/tddy-daemon/tests/remote_managed_worktree_cross_host_acceptance.rs`: two real daemons over the LiveKit testkit (Docker or `LIVEKIT_TESTKIT_WS_URL`), `#[serial]`.
  - Fixtures: `split_hosts()`, `split_hosts_with_agent_claude_binary`, `split_hosts_with_codebase_provisioner_and_agent_binary`, `a_daemon(ws_url, instance_id, os_user, repo, claude_stub, provisioner)`, `serve_rpc_participant`, `wait_until_discovered`, `a_split_session_request()`, `a_sandboxed_split_session_request()`, `session_directories_on`, `sessions_on`, `create_test_repo_with_origin`, `a_claude_stub`, `write_livekit_daemon_yaml`, and the signing/token helpers (`a_caller_token`, `a_caller_token_signed_by`).
  - Teardown tests: `a_worktree_failure_on_the_codebase_daemon_leaves_no_session_behind`, `a_failed_agent_spawn_tears_down_the_workspace_session_on_the_codebase_daemon`.
- `packages/tddy-daemon/tests/session_attach_cross_host_acceptance.rs`: `two_daemons()`, `two_daemons_forwarding_within(secs)`, `stream_start_session_forwards_to_the_peer_that_runs_the_session` (the forwarded StreamStartSession end-to-end, matching all four event variants), and `stream_start_session_on_the_peer_reports_progress_while_staged_bytes_cross_from_this_host`.
- `packages/tddy-daemon/tests/daemon_rpc_participant/mod.rs`: `serve_daemon_rpc_participant(ws_url, token, &Arc<DaemonSessionHost>)` serving `tddy_session_lifecycle::test_util::daemon_rpc_roster`, and `with_the_livekit_room_ports`.
- Other suites in the same area (names only): `tddy-daemon/tests/split_session_resume_acceptance.rs`, `session_room_cross_host_acceptance.rs`, `session_agent_remote_acceptance.rs`; `tddy-daemon-rpc/tests/remote_managed_worktree_exec_tool_acceptance.rs`, `sandboxed_codebase_lifecycle_exec_tool_acceptance.rs`; `tddy-daemon-livekit/tests/livekit_peer_daemons_acceptance.rs`, `forward_to_peer_shared_registry.rs`, `forwarded_rpc_is_stamped_by_the_receiver.rs`.
- `tddy-livekit-testkit` (`src/livekit_testkit.rs`): `LiveKitTestkit::start()`, `unique_room(prefix)`, `get_ws_url()`, `generate_token(room, identity)`, `remove_participant(room, identity)`. `remove_participant` can trigger `on_peer_disconnected`.
- `tddy-rpc/tests/server_engine_peer_disconnect.rs`: pins the contract "nothing further published after `on_peer_disconnected`". `tddy-rpc/tests/server_engine.rs` builds raw `RpcRequest{.. abort: false ..}`, which an abort-frame test could reuse.
- Web: Cypress `SessionStartAndIndexingProgress.cy.tsx`, `CreateSessionClonePassphraseMount.cy.tsx`, `ServerStreamFallback.cy.tsx`, `CreateSessionAttachmentProgress.cy.tsx`. `tddy-livekit-web/cypress/component/transport.cy.tsx:141` has an AbortSignal test, known to be partially broken (todo 2026-08-15).

### Exploration 5: Web session-creation UI — 2026-10-24

**Agent**: parent Grep/Read (the Explore subagent for this pass was terminated by an API safeguard
error before returning; nothing from it was used)
**Scope**: how the create pane consumes `StreamStartSession`, renders progress, and cancels

### Sequence

1. Read `packages/tddy-web/src/hooks/useSessionAttachments.ts:320-440` — `startSessionStreamed`.
2. Grep `startPhase|StartPhase_Step` in `src`, `cypress` (excluding `src/gen`).
3. Read `src/components/sessions/CreateSessionStartPhase.tsx`, `cypress/component/SessionStartAndIndexingProgress.cy.tsx:1-245`, `cypress/support/pages/createSessionPage.ts:360-400`, `CreateSessionPane.tsx:225-260, 430-500`.
4. Grep `cancelledRef`.
5. Grep `startSessionStreamed|client.startSession`; read `src/components/sessions/useCreateSessionSubmit.ts:1-140`, `CreateSessionActions.tsx`.
6. `awk` the `StartSessionRequest` field list and `rpc` list from `packages/tddy-service/proto/session.proto`; read the `requested_session_id` doc.

### Grep / glob

| Tool | Pattern | Scope | Notable hits |
|---|---|---|---|
| Grep | `startPhase\|StartPhase_Step` | `tddy-web/src`, `cypress` | `CreateSessionPane.tsx:241,461`; `CreateSessionStartPhase.tsx`; `useSessionAttachments.ts:22,88,231,423`; `SessionStartAndIndexingProgress.cy.tsx:129-146,244`; `CreateSessionAttachmentProgress.cy.tsx:254,271`; `createSessionPage.ts:382` |
| Grep | `startSessionStreamed\|.startSession(` | `tddy-web/src` | only `useCreateSessionSubmit.ts:85` — **always streamed** |
| Grep | `cancelledRef` | hook, pane | set only on unmount (`useSessionAttachments.ts:242-246`) |

### Inspected files

#### `packages/tddy-web/src/hooks/useSessionAttachments.ts:371-418`

```ts
/**
 * Consumed with a `cancelled` flag rather than an `AbortSignal`: the LiveKit transport accepts a
 * signal for server-streaming calls and never reads it, so aborting would look correct here and be
 * a no-op in production. A stream that ends without a result, or errors, is a failed creation.
 */
const startSessionStreamed = async (request) => {
  for await (const event of client.streamStartSession(request)) {
    if (cancelledRef.current) return null;
    if (event.event.case === "attachmentProgress") { … phase: "materializing" … }
    else if (event.event.case === "result") { result = event.event.value; }
    else if (event.event.case === "phase") { BEGIN → setStartPhase(step); END → clear if current }
  }
  // finally: setStartPhase(null) unless cancelled
};
```

`keepAlive` is ignored implicitly.

#### `packages/tddy-web/src/components/sessions/CreateSessionStartPhase.tsx`

```tsx
const STEP_TEXT: Record<StartPhase_Step, string> = {
  [StartPhase_Step.UNSPECIFIED]: "",
  [StartPhase_Step.WORKTREE]: "Creating worktree…",
  [StartPhase_Step.SEMANTIC_INDEX]: "Indexing (semantic)…",
  [StartPhase_Step.AGENT]: "Starting agent…",
};
export function CreateSessionStartPhase({ step }) {
  if (step === null || step === StartPhase_Step.UNSPECIFIED) return null;
  return <p data-testid="create-session-start-phase" className="text-sm text-muted-foreground">{STEP_TEXT[step]}</p>;
}
```

#### `packages/tddy-web/src/components/sessions/CreateSessionActions.tsx`

Cancel button `data-testid="create-session-cancel-btn"`, `onClick={onCancel}` (closes the pane),
`disabled={submitting}`.

#### `packages/tddy-web/cypress/component/SessionStartAndIndexingProgress.cy.tsx`

`aBaselineBackend()` (every unary the form issues), `aPhase(step, boundary)`, `aGate()` to hold a stub
generator mid-stream, `aHostHoldingTheStartInItsWorktreeStep(gate)` implementing
`streamStartSession` as an `async *` generator, `mountTheCreatePane(backend)` with `mountWithRpc` +
`SelectedDaemonProvider`. Its doc comment ("Today the form streams a start only when something is
attached") is **stale** — the submit path always streams.

#### `packages/tddy-service/proto/session.proto:555-565`

`requested_session_id = 33` — "Only honoured for session_type = "workspace"; sending it with any other
type is a request error". Exists so a split's originator can name, and delete, the workspace session
even when the forward never answers.

### Findings

- Cancel cannot be an `AbortSignal`; it must be an explicit RPC that names the start.
- A caller-chosen start id is the natural handle; today only workspace sessions accept one.
- Progress UI is a single text line; it needs a step list and a clone progress row.
- The existing Cancel button is disabled while submitting; it becomes "cancel the start" in that state.
- Cypress fixtures to reuse: `aBaselineBackend`, `aGate`, `aPhase`, `mountTheCreatePane`,
  `createSessionPage.startPhase()`.

### Exploration 6: Deferred-work cross-check and both hosts' timeout settings — 2026-10-24

**Agent**: parent Bash/Grep
**Scope**: code issues and TODOs in the change's path; why 30 s

### Sequence

1. `ls packages/<pkg>/docs/code-issues/` + `grep -l 'Claimed by:'` for tddy-spawn, tddy-supervisor, tddy-agent-launch, tddy-session-split, tddy-daemon-kernel, tddy-daemon-livekit, tddy-daemon, tddy-web, tddy-service, tddy-rpc, tddy-daemon-rpc.
2. Read the Location/Metrics/Restructure/Status of seven issues in the path.
3. `grep -rl -iE 'supervisor|clone|forward.*(timeout|deadline)|keep.?alive|StreamStartSession|spawn.worker|pre_exec|orphan|split start|start progress|cancel' docs/dev/todo/`.
4. Read eight TODO bodies (listed below).
5. Read `docs/ft/daemon/clone-ssh-passphrase.md`; list `docs/ft/daemon`, `docs/ft/web` and their `1-WIP`.
6. `grep spawn_worker_request_timeout_secs|peer_forward_timeout_secs` in `~/.tddy/desktop.yaml` and remote `/etc/tddy/daemon.yaml`.

### Grep / glob

| Tool | Pattern | Scope | Notable hits |
|---|---|---|---|
| Glob | `docs/code-issues/` | 11 packages | none claimed; no dir for tddy-spawn, tddy-daemon, tddy-rpc |
| Grep | areas above | `docs/dev/todo/` | 72 files; relevant ones read below |
| Grep | timeouts | both configs | desktop 300 + 30; remote 30 |

### Inspected files

- `docs/dev/todo/2026-10-03-start-phases-and-code-index-warm-skip-sandboxed-tool-and-split-starts.md` — split starts report no `StartPhase`.
- `docs/dev/todo/2026-10-17-a-timed-out-clone-leaves-a-partial-destination-that-later-starts-skip.md` — abandoned clone child keeps running; `test -e` skips a half-written destination next time; fix needs temp-dir+rename or remove-on-failure and a killable child.
- `docs/dev/todo/2026-09-15-rpcservice-has-no-cancellation-surface.md` — no cancellation parameter on `RpcService`; warm-index workaround = `TaskRegistry` + token checked inside blocking loops.
- `docs/dev/todo/2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md` — a call to a vanished participant hangs; streams must not inherit a unary timeout.
- `docs/dev/todo/2026-09-10-supervisor-spawn-delegation-keeps-tddy-supervisor-in-the-daemon.md` — two fail-closed integration tests mount the daemon with a declared-but-unreachable supervisor.
- `docs/dev/todo/2026-09-10-tddy-spawn-modules-are-over-budget-and-not-yet-reusable.md` — `spawner.rs` 2,152 → now 2,287 lines; suggested seams.
- `docs/dev/todo/2026-10-17-clone-passphrase-prompt-verification-gaps.md` — no forwarded clone-bearing start over a real LiveKit room; supervised clone with askpass never run.
- `docs/dev/todo/2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md` — the same pre_exec privilege drop makes per-user probes fail under the supervisor; "needs a deployment decision".
- `docs/ft/daemon/clone-ssh-passphrase.md` — AC "A forwarded start, unary or streamed, is bounded by the split-start budget, not the 30-second forward deadline"; keep-alive every 5 s while provisioning.

### Findings

- No 🚧 claimed issue: no wait-or-proceed fork.
- The split-start-phases TODO is closed by this change; the partial-destination TODO is in its path.
- The 30 s was the remote's `spawn_worker_request_timeout_secs: 30`, answered as `DEADLINE_EXCEEDED`
  and misclassified by the desktop as `ForwardDeadline`.
