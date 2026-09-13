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

`tddy-daemon`'s `tool_catalog_sync` guard test stays, because it guards a pair that has *not*
collapsed — this catalog against `tddy_sandbox::workspace_exec_tool_names`, the allowlist a
sandboxed `claude` is spawned with.

## Tools

| Tool | Behaviour |
|------|-----------|
| `Read` | Read a file under the worktree root (range support). |
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

## Callers

- **`tddy-daemon`** — imports the crate via a `pub use tddy_tool_engine as tool_engine;`
  re-export so legacy `tool_engine::execute_tool` / `execute_tool_with_env` call sites are
  unchanged; `ListExecTools` maps `tddy_tool_engine::ToolDef` → proto `ToolDef` at the RPC
  boundary. The sandbox-allowlist sync test lives in `src/tool_catalog_sync.rs`.
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
- `catalog::tests` — every catalog entry has a unique, non-empty name.
