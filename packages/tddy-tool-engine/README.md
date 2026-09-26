# tddy-tool-engine

Shared, generic tool-dispatch engine used by **`tddy-daemon`** and **`tddy-coder`** to execute
the operator-facing tool catalog (`Read` / `Write` / `StrReplace` / `Delete` / `Grep` /
`Glob` / `Shell` / `Await` / `ReadLints` / `SemanticSearch`) against a session's worktree.

It is path-contained against a caller-supplied **`worktree_root`** (every path-resolving tool
rejects targets that escape the root) and backed by **[`tddy-task`](../tddy-task/)** for
long-running background jobs (e.g. `Shell` with `block_until_ms = 0`).

## Public API

- `execute_tool(worktree_root, tool_name, args_json, registry, session_id) -> ToolOutcome`
  — dispatch one tool call. `registry: &TaskRegistry` holds background jobs spawned by
  `Shell`/`Await`; `session_id` tags jobs.
- `execute_tool_with_env(...)` — variant that forwards an environment map to spawned shells.
- `Shell`, `LocalShell`, `RemoteShell`, `session_shell`, `execute_tool_on_shell` — pick local disk
  vs OpenSSH (`BatchMode=yes`) for managed sessions with `ssh_config_host`; remote dispatch covers
  the full exec catalog with path containment against the remote worktree root.
- `tool_catalog() -> Vec<ToolDef>` — the canonical catalog. `ToolDef { name, description,
  input_schema_json }` is the engine's own struct (independent of `tddy-service` proto);
  callers map it to their RPC type at the boundary.
- `ToolOutcome` — the execution result; for background jobs it carries `job_id` and
  `job_running`.
- `dynamic_proxy::is_native_tool_denied_in_remote_mode(tool_name) -> bool` — whether a native
  mutation tool (`Write` / `Edit` / `NotebookEdit`) must be hard-**denied** while the agent runs
  against a remote codebase, where the local working directory is not the worktree being edited. A
  denial rather than an absence: the agent needs to be told it was refused, and the replacement is
  this crate's own `Write` against the real worktree.

## One catalog

`tool_catalog()` is the only exec-tool catalog in the workspace. `tddy-tools` used to hold a
hand-copied clone of it in its MCP shape, with matched guard tests in both crates paying to keep the
two in step; it now derives its `RemoteToolDef`s from this function at the single point that needs
that shape, the same way it derives the `Lsp*` tools from `tddy_lsp_executor`.

The MCP shape itself — `RemoteToolDef`, `build_dynamic_tool_list`, `dynamic_tool_router`,
`dispatch_dynamic_tool` — stays in `tddy-tools`, which is the crate that speaks MCP. Putting it here
would mean `rmcp` in a crate every workspace-session host links, and `dispatch_dynamic_tool`
additionally resolves the call against the session's live agent roster, which is a `tddy-service`
concern. Advertisement is **not** filtered by that roster: a tool an agent has taken over is still
advertised and refused at dispatch, because `--allowedTools` is fixed when `claude` spawns.

## `exec_tools.ExecToolService`

This crate also **serves** the ten tools it defines, at `exec_tools.ExecToolService`
(`build_exec_tool_entry`). The daemon, session rooms, LiveKit participants, local Unix socket, and
`tddy-coder`'s session participant register that entry beside the other unbundled families.
`tddy-session-tool-client` and in-jail relays call `ExecuteTool` / `StreamExecuteTool` on that
coordinate.

The `ExecToolHandler` trait and its `ExecToolServiceImpl` adapter live in `exec_tool_service.rs`,
and the session tool-call log in `tool_call_log.rs`. `ListExecTools` maps `tool_catalog()` to proto
`ToolDef` at the wire boundary; `ListSessionToolCalls` reads the session JSONL log. The daemon's
handler is `tddy_daemon_rpc::ExecToolRpcHandler`
([tddy-daemon-rpc](../tddy-daemon-rpc/docs/architecture.md)), and the suites that drive the service
through it — `tool_call_log_acceptance.rs` among them — live in that crate.

## Tools

| Tool | Behaviour |
|------|-----------|
| `Read` | Read a file under the worktree root. `offset` (0-based first line) and `limit` select a line window; the result is `{content, truncated, total_lines}`, where `truncated` says whether lines follow the window and `total_lines` is the file's own length. A window past end-of-file is empty rather than an error. **No default cap**: a bare `Read` returns the whole file byte for byte, trailing newline included. |
| `Write` | Create/overwrite a file under the root. |
| `StrReplace` | Exact string replacement in a file under the root. |
| `Delete` | Delete a file under the root. |
| `Grep` | ripgrep search under the root. |
| `Glob` | Glob file search under the root. |
| `Shell` | Run a shell command under the root; foreground (bounded) or background
  (`block_until_ms = 0` → registers in `TaskRegistry`, returns `job_id`). |
| `Await` | Block on / poll a background job in the `TaskRegistry`. |
| `ReadLints` | Read lint diagnostics for the worktree. |
| `SemanticSearch` | Semantic code search under the root. |

All path-resolving tools are contained: a target that resolves outside `worktree_root` is
rejected.

### The 200-line cap is not here

A subagent's reads are bounded at 200 lines, and that bound lives one layer up, in
`tddy_discovery::subagent`: the Local path applies it to bytes already in hand, and the Managed
path puts it in the request *before* the file crosses the wire, because a cap applied after the
transfer bounds the context but not the wire. The two defaults are deliberately different — this
crate decides what a tool call returns, that one decides how much of it an agent may pull into a
model context.

## Every child process is contained

`contained_shell` is the one way this crate starts a process: the blocking `Shell` path, a
background `ShellTaskBody`, `LocalShell::run`, and the helper binaries a tool spawns directly
(`Grep`'s `rg`) all go through it. A child started any other way inherits two defects that together
cost a session:

- `tokio::process::Command::output()` sets stdout and stderr but, unlike its `std` counterpart,
  leaves **stdin inherited**. Inside a jail that stdin is `tddy-sandbox-runner --stdio`'s tool-IPC
  request pipe, so a command reading standard input becomes a second reader on the daemon→jail
  channel and consumes frames meant for the runner.
- `tokio::time::timeout` only stops *waiting*. The command keeps running, and keeps reading, long
  after the caller has been told it timed out.

So a contained command gets `/dev/null` for standard input and its own **process group**, and on
overrunning its budget the whole group is signalled — `SIGTERM`, then `SIGKILL` after a short grace
— because the descendants of `( sleep 1; touch marker ) & wait` outlive the `sh` the engine
started.

## Callers

- **`tddy-daemon`** — imports the crate via a `pub use tddy_tool_engine as tool_engine;`
  re-export so legacy `tool_engine::execute_tool` / `execute_tool_with_env` call sites are
  unchanged; registers `build_exec_tool_entry` on every transport.
- **`tddy-coder`** — `CoderSessionToolExecutor` holds the session's `worktree_root` (the
  coder's `agent_working_dir`) and a per-session `tddy_task::TaskRegistry`;
  `coder_session_tool_catalog()` mirrors the shared catalog. The `ToolExecutor` seam is
  `async` to align with the engine's async `execute_tool`. See
  [Session Participant RPC & Metadata](../../docs/ft/coder/session-participant-rpc.md).

## Dependencies

`tddy-task`, `glob`, `bytes`, `serde_json`, `tokio`, `async-trait`, `log`.

## Tests

- `tests/execute_tool_acceptance.rs` — Write→Read round-trip, path-traversal rejection,
  unknown-tool honest error, catalog lists every dispatched tool.
- `tests/shell_containment_red.rs` — a command cannot read the parent's standard input, on the
  blocking path and on a background job; a command that outlives its budget leaves no descendant.
  The two stdin cases **re-exec the test binary** with fd 0 bound to an open pipe holding data:
  under `cargo test` the harness's own stdin is already at end of file, so a case running `cat`
  directly passes against the live defect.
- `tests/read_window_engine_red.rs` — the `Read` window: only the requested lines, a window
  reaching end-of-file is not `truncated`, a window past the end is empty rather than an error, and
  a bare read returns the file unchanged (which catches a lines-and-rejoin implementation dropping
  the trailing newline).
- `catalog::tests` — every catalog entry has a unique, non-empty name.
