# Discovery Agent

## Summary

The Discovery sub-agent is a lightweight repository-exploration tool that locates relevant code and
returns compact `path:line-start-line-end` citations. It is driven by
**`microsoft/FastContext-1.0-4B-RL`** — a ~4B parameter model post-trained for codebase navigation —
served over an OpenAI-compatible HTTP API.

Discovery can operate in two modes:

- **Local mode**: runs read-only file tools (`READ`/`GLOB`/`GREP`) against the local filesystem.
- **Remote mode**: routes the same tools through the existing `tddy-tools` relay →
  [`ExecuteTool`](../daemon/remote-codebase-mode.md) path, so the agent can explore a codebase hosted
  on a remote `tddy-daemon` without a local checkout.

Citations produced by Discovery are mapped onto `DiscoveryData.relevant_code` entries
(`RelevantCode{path, reason}`) and can be consumed by other workflow steps (e.g. the planning step).

## Background: `tddy-graph` extraction

Discovery's multi-turn tool-calling loop needs a graph-runner that is independent of `tddy-core`.
As a prerequisite, the repo's custom "lang-graph" implementation
(`tddy-core/src/workflow/{graph,context,session,runner,hooks,task}`) is extracted into a new standalone
crate **`tddy-graph`** (no `tddy-core` dependency). This is a pure, behavior-preserving refactor;
all existing consumers remain source-compatible via a re-export shim.

## Model: `microsoft/FastContext-1.0-4B-RL`

- **Base**: Qwen3-4B-Instruct, post-trained SFT + GRPO RL. ~4B params.
- **Role**: dedicated repository-exploration sub-agent. Input = natural-language query about the codebase.
- **Tools (exactly 3, read-only)**: `READ` (line-numbered file contents), `GLOB` (path discovery by
  glob pattern), `GREP` (regex search across files).
- **Output**: a `<final_answer>` block listing `path:line-start-line-end` citations, after up to
  `--max-turns` turns. The model forces a final answer at the cap.
- **Serving**: [SGLang](https://github.com/sgl-project/sglang) with `--tool-call-parser qwen
  --context-length 262144 --dtype bfloat16 --trust-remote-code`. vLLM / transformers also supported.
- **API**: the serving layer parses Qwen XML tool syntax internally and exposes a standard OpenAI
  `/v1/chat/completions` API. No XML parsing is needed client-side — the client sees standard
  `tool_calls` JSON objects.
- **Default endpoint**: `http://localhost:30000` (configurable via `fastcontext_url` in config YAML
  and the `--fastcontext-url` CLI flag).
- **Model id**: defaults to `microsoft/FastContext-1.0-4B-RL`; override via `fastcontext_model` in
  config YAML or `--fastcontext-model` to target any other OpenAI-compatible endpoint or model tag —
  including a locally-served model through [Ollama](https://ollama.com)'s
  `/v1/chat/completions` API (`--fastcontext-url http://localhost:11434 --fastcontext-model
  <your-ollama-tag>`). The model id is sent verbatim in each chat-completion request body; no other
  backend behavior changes.
- **Sources**:
  - <https://huggingface.co/microsoft/FastContext-1.0-4B-RL>
  - <https://github.com/microsoft/fastcontext>

## Architecture

```
tddy-coder
  └─ create_backend("fastcontext") → SharedBackend::from_arc(FastContextBackend)
       │
       └─ tddy-discovery crate
            ├─ openai.rs   — /v1/chat/completions HTTP client (reqwest)
            ├─ tools.rs    — ToolExecutor { Local | Remote }
            │     Local: std::fs / glob / regex
            │     Remote: POST ExecuteTool with RemoteToolEnv
            ├─ backend.rs  — FastContextBackend: CodingBackend (multi-turn loop)
            └─ discovery.rs — citation lines → DiscoveryData
```

The `FastContextBackend` implements the `CodingBackend` trait from `tddy-core` and is wired via
`SharedBackend::from_arc` — no `AnyBackend` enum variant is required.

Remote routing reuses the existing `ExecuteTool`/`ListExecTools` RPCs documented in
[remote-codebase-mode.md](../daemon/remote-codebase-mode.md). The `RemoteToolEnv` (daemon URL, session
token, etc.) is carried on the `InvokeRequest.remote` field — not read from `TDDY_REMOTE_*` process env
(that path is for the subprocess MCP agent; the Discovery loop is native Rust inside the workflow process).

## User Story

As a developer, I want to run a Discovery query against my codebase (local or remote) and receive a
compact list of `path:line-range` citations so that subsequent planning and implementation steps start
from the right files without requiring a manual search.

## Acceptance Criteria

### Phase A — `tddy-graph` extraction (pure refactor)

1. The entire workspace compiles after extraction — all existing tests pass unchanged. No new
   dependencies are required for Phase A.
2. `tddy-graph` exposes `graph`, `context`, `session`, `task`, `hooks`, and `runner` modules with no
   dependency on `tddy-core` or any other `tddy-*` crate.
3. All types previously at `tddy_core::workflow::{graph,context,session,task,hooks,runner}::*`
   remain accessible at those same paths (via a re-export shim).
4. `BackendInvokeTask` remains accessible at `tddy_core::workflow::task::BackendInvokeTask`.
5. `FlowRunner::run` calls `on_enter_task` before each task and `on_exit_task` on **every** exit
   after the task runs — including the error arm, the `WaitForInput` and `End` early returns, and the
   no-successor pause path. No exit path is skipped.
6. The concrete `RunnerHooks` impls in `tddy-workflow-recipes` correctly wire sinks via `on_enter_task`
   and `on_exit_task` (replacing the removed `agent_output_sink`/`progress_sink` trait methods).

### Phase B/C — FastContext backend + Discovery agent

7. `FastContextBackend::name()` returns `"fastcontext"`.
8. Given a mock `/v1/chat/completions` server, `FastContextBackend::invoke` runs the multi-turn loop:
   - On each turn, parse `tool_calls` from the response; execute the corresponding tool; append the
     result as a `tool`-role message.
   - When the response contains `<final_answer>`, terminate the loop and return the citations.
   - When `max_turns` is reached with no `<final_answer>`, return a defined terminal result (no infinite loop).
9. `ToolExecutor::Local` executes `READ`/`GLOB`/`GREP` against the local filesystem. File not found,
   no matches, and empty directory results are returned as valid (non-error) results.
10. `ToolExecutor::Remote` POSTs to `{daemon_url}/connection.ConnectionService/ExecuteTool` with the
    correct envelope (`session_token`, `session_id`, `tool_name`, `args_json`, `daemon_instance_id`
    from `RemoteToolEnv`). `is_error:true` responses surface as errors — no silent fallback.
11. The executor mode is selected from `InvokeRequest.remote`: `Some(RemoteToolEnv)` → Remote,
    `None` → Local.
12. `citation_lines_to_discovery_data` maps `path:line-start-line-end` strings to
    `RelevantCode{path, reason}` entries in `DiscoveryData`. Malformed lines are excluded (no panic,
    no fallback that silently includes garbage).

### Phase D — surface wiring

13. `create_backend("fastcontext", ...)` in `tddy-coder` returns a `SharedBackend` wrapping a
    `FastContextBackend` with `name() == "fastcontext"`.
14. `--agent fastcontext` is accepted as a valid CLI argument in `tddy-coder`.
15. `dev.daemon.yaml::allowed_agents` includes `fastcontext`.
16. `fastcontext_url` is configurable in the YAML config and defaults to `http://localhost:30000`.
17. `fastcontext_model` is configurable via CLI flag and YAML config, defaults to
    `microsoft/FastContext-1.0-4B-RL`, and is threaded through `create_backend` to
    `FastContextBackend::new` unchanged — enabling any OpenAI-compatible model tag, including
    locally-served models via Ollama.

## Non-goals (out of scope for v1)

- FastContext server provisioning, deployment, or model download automation.
- Citation-mode toggle exposed as a CLI flag (the backend always uses citation mode).
- `max_turns` tuning UX beyond the config file default.
- Sharing a single `ExecuteTool` HTTP client between `tddy-tools` and `tddy-discovery` beyond the
  envelope-construction helper on `RemoteToolEnv` (two independent reqwest call sites is acceptable given
  the different invocation contexts — subprocess env vs. struct-carried env).
- Integration of Discovery output into the `plan` workflow goal (follow-up work; `DiscoveryData` is
  already the right output type).
- Web UI for Discovery queries.
