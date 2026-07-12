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
