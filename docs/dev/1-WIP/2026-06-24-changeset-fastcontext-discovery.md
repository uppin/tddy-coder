# Changeset: FastContext Discovery Agent (Phase B/C/D)

**Date:** 2026-06-24
**Branch:** `suave-cougar`
**Status:** WIP
**Depends on:** `2026-06-24-changeset-tddy-graph-extraction.md`

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Write acceptance tests
- [x] Write unit / integration tests
- [x] Implement
- [x] All tests pass
- [ ] Wrap

## Summary

Build the FastContext Discovery agent on top of `tddy-graph`. The agent maps/locates relevant code and
returns compact `path:line-start-line-end` citations, runnable against a **local** or **remote**
codebase. Remote routing reuses the existing `ExecuteTool`/`ListExecTools` RPCs from
[remote-codebase-mode.md](../../ft/daemon/remote-codebase-mode.md) — no new RPCs.

## Packages affected

- **new** `packages/tddy-discovery` — `FastContextBackend: CodingBackend` + multi-turn OpenAI loop +
  `ToolExecutor` (local & remote) + citation → `DiscoveryData` mapping.
- `packages/tddy-core` — `RemoteToolEnv` gains an envelope-construction helper for `ExecuteTool` POST
  body (envelope construction shared; the actual HTTP call stays in `tddy-discovery`).
- `packages/tddy-coder` — `create_backend("fastcontext", ...)` arm; `tddy-discovery` dep; config
  `fastcontext_url`/`max_turns`; CLI `--agent fastcontext`; `dev.daemon.yaml` `allowed_agents`.
- Root `Cargo.toml` — adds `packages/tddy-discovery` to `members`.

## New dependencies

- `packages/tddy-discovery/Cargo.toml`:
  - **runtime**: `reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json"] }`
    (already present in 5 workspace crates; zero new workspace-level vetting required).
  - **runtime**: `serde`, `serde_json`, `glob`, `regex`, `tokio`, `async-trait`, `log` (all already
    workspace deps).
  - **dev**: `wiremock` — mock HTTP server for OpenAI loop and ExecuteTool tests. Check workspace lock
    before adding; if absent add `wiremock = "0.6"` (or latest) as a dev-dep of `tddy-discovery` only.
    Fallback: hand-rolled `axum` 0.8 test server (already a workspace dep in `tddy-coder`).
  - **dev**: `tddy-testing-commons` (temp file helpers).

## `tddy-discovery` module layout

```
packages/tddy-discovery/
  Cargo.toml         (deps: tddy-core, reqwest, serde, serde_json, glob, regex, tokio, async-trait, log)
  src/
    lib.rs           (pub use backend::FastContextBackend; pub use discovery::citation_lines_to_discovery_data)
    openai.rs        — /v1/chat/completions request/response structs; reqwest POST; base URL injectable
    tools.rs         — ToolExecutor { Local(path), Remote(RemoteToolEnv) }
                       READ: std::fs / daemon Read
                       GLOB: glob crate / daemon Glob
                       GREP: regex / daemon Grep
                       result_json shapes: Read→{content}, Grep→{matches:[ripgrep-json]}, Glob→{paths:[]}
    backend.rs       — FastContextBackend: CodingBackend
                       invoke(InvokeRequest) → runs multi-turn loop → InvokeResponse{output: citations}
                       name() → "fastcontext"
    discovery.rs     — parse "path:N-M" citation lines → Vec<RelevantCode>; malformed lines excluded
```

## Remote tool routing detail

`RemoteToolEnv` (already in `tddy-core/src/backend/mod.rs:288`) gains a helper method:
```rust
pub fn execute_tool_url(&self) -> String { ... }  // {daemon_url}/connection.ConnectionService/ExecuteTool
pub fn execute_tool_request_body(&self, tool_name: &str, args_json: &str) -> serde_json::Value { ... }
```
The actual HTTP POST (reqwest) is owned by `tddy-discovery/src/tools.rs`. `tddy-tools`'
`dispatch_dynamic_tool` (server.rs:482) stays unchanged — it reads from `TDDY_REMOTE_*` env vars and
is the correct path for the subprocess MCP agent.

## Surface wiring (`tddy-coder`)

### `src/run.rs::create_backend` (currently lines 1927-1969)

Add arm:
```rust
"fastcontext" => {
    let url = config.fastcontext_url.clone().unwrap_or_else(|| "http://localhost:30000".to_string());
    let max_turns = config.fastcontext_max_turns.unwrap_or(6);
    let backend = tddy_discovery::FastContextBackend::new(url, model, max_turns);
    SharedBackend::from_arc(Arc::new(backend))
}
```

### `src/config.rs`

Add fields: `pub fastcontext_url: Option<String>`, `pub fastcontext_max_turns: Option<u32>`.

### `dev.daemon.yaml::allowed_agents`

Append: `- id: fastcontext  label: "FastContext (microsoft/FastContext-1.0-4B-RL)"`.

### `backend_from_label` / `default_model_for_agent` / `backend_selection_question`

Add `fastcontext` entries. `value_parser` list for `--agent` in `run.rs` gains `"fastcontext"`.

## Tests

### Acceptance test (`packages/tddy-coder`)

- `create_backend_returns_a_fastcontext_backend_for_the_fastcontext_agent_string`

### Unit / integration tests (`packages/tddy-discovery`)

**`src/openai.rs`** (wiremock):
- `parses_tool_calls_from_a_chat_completion_response`
- `serializes_tools_and_messages_into_the_request_body`

**`src/backend.rs`** (wiremock mock `/v1/chat/completions`):
- `invoke_runs_the_multi_turn_loop_until_final_answer`
- `invoke_stops_at_max_turns_when_no_final_answer`
- `invoke_reports_name_as_fastcontext`

**`src/tools.rs`**:
- `local_read_tool_returns_file_content`
- `local_glob_tool_returns_matching_paths`
- `local_grep_tool_returns_matching_lines`
- `remote_executor_posts_execute_tool_and_maps_read_content`
- `remote_executor_maps_grep_matches_shape`
- `remote_executor_maps_glob_paths_shape`
- `remote_executor_surfaces_is_error_responses`
- `executor_selects_remote_mode_when_remote_tool_env_present`
- `executor_selects_local_mode_when_remote_tool_env_absent`

**`src/discovery.rs`**:
- `maps_citation_lines_into_relevant_code_entries`
- `ignores_malformed_citation_lines`
- `populates_discovery_data_fields_from_final_answer`

## Feature doc

[docs/ft/coder/discovery-agent.md](../../ft/coder/discovery-agent.md) (Phase B/C/D acceptance criteria 7–16)
