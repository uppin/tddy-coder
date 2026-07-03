# Remote-Codebase Mode

> User-facing note: this mechanism is presented to users as **managed-codebase mode** (see
> [managed-codebase-subagents.md](../coder/managed-codebase-subagents.md), which also adds
> pluggable discovery subagents on top of it). The identifiers, RPCs, and env vars documented
> below (`RemoteToolEnv`, `TDDY_REMOTE_*`, `ExecuteTool`, etc.) keep their existing names for wire
> stability — only the prose a human/agent reads was renamed.

## Summary

Remote-codebase mode lets `tddy-coder` run an agent against a codebase hosted on a **remote
`tddy-daemon`**, without requiring a local checkout. The agent runs locally (inside `claude`), but
every file read, write, grep, glob, shell command, and semantic search is executed by the remote
daemon against a real git worktree. The agent never touches the local filesystem for code ops.

The key architectural pieces:

1. **Remote daemon — workspace session + tool execution engine**: a new `session_type:"workspace"`
   creates a git worktree with no PTY or agent. Two new RPCs — `ExecuteTool` and `ListExecTools` —
   let callers run cursor-compatible file/shell tools against the worktree and discover the tool catalog.
2. **Local relay daemon (`tddy-daemon --relay`)**: a lightweight, lazily-started, single-instance
   `tddy-daemon` that runs locally, joins the LiveKit common room, and forwards RPCs to the remote
   daemon peer. It holds the persistent LiveKit connection across many short-lived `claude` invocations
   and self-stops after an idle timeout.
3. **`tddy-tools` dynamic MCP proxy**: the agent's `tddy-tools --mcp` server discovers the tool catalog
   from the relay at startup via `ListExecTools` and re-exposes those tools as `mcp__tddy-tools__*`
   entries, with no hardcoded remote-tool list. Each tool call is forwarded through the relay to the
   remote daemon via `ExecuteTool`.
4. **`tddy-coder` remote mode**: shells out to `tddy-tools remote …` subcommands for session
   bootstrap, context sync, and tool-name discovery; the agent's working directory is a read-only
   temp dir with synced CLAUDE.md/AGENTS.md/skills and an appended "remote codebase" notice.

## User Story

As a developer, I want to start a `tddy-coder` session against a remote codebase (managed by a
remote `tddy-daemon`) so that my agent can plan, read, write, and test code in a remote git worktree
— without checking out the repository locally.

## Acceptance Criteria

### Remote daemon: workspace session

1. `StartSession` with `session_type:"workspace"` and a valid `project_id` creates a git worktree
   (branch from base), writes `.session.yaml` with `session_type: workspace` and a `repo_path`,
   and returns a `session_id`. **No PTY is spawned; no agent process is started.**
2. `ConnectSession` and `ResumeSession` against a workspace session return empty LiveKit credentials
   (the workspace has no terminal to connect to).
3. `DeleteSession` for a workspace session removes the session directory and the worktree.

### Remote daemon: tool execution

4. `ListExecTools` returns a list of `ToolDef` records — one per supported tool — each with a non-empty
   `name`, `description`, and a valid JSON Schema in `input_schema_json`.
5. `ExecuteTool` with `tool_name:"Read"` and a valid path returns the file contents as
   `result_json:{content:"..."}`. `is_error` is false.
6. `ExecuteTool` with `tool_name:"Write"` creates or overwrites a file in the worktree. A subsequent
   `ExecuteTool("Read")` on the same path returns the written content.
7. `ExecuteTool` with a path that escapes the worktree root (e.g. `../../etc/passwd`) returns an
   RPC error with `permission_denied` status — not an `is_error` tool result.
8. `ExecuteTool` with an unknown `tool_name` returns `is_error:true` and a descriptive `error_message`,
   not an RPC error.
9. `ExecuteTool` with `tool_name:"Shell"` and `block_until_ms:0` (background) returns immediately
   with a non-empty `job_id` and `job_running:true`.
10. `ExecuteTool` with `tool_name:"Await"` and the `job_id` from criterion 9 blocks until the
    background shell completes and returns the exit code.

### Remote daemon: connect-by-id

11. `ExecuteTool` against the `session_id` of an existing `claude-cli` session (which has a real
    `repo_path`) executes the tool against that session's worktree — not just `workspace` sessions.

### Local relay daemon (`tddy-daemon --relay`)

12. `tddy-daemon --relay` starts with a config that has **no** `web_bundle_path` and an empty `users`
    list; the daemon starts successfully and binds its Connect HTTP port.
13. An `ExecuteTool` call to the relay daemon, where `daemon_instance_id` matches a remote peer on the
    LiveKit common room, is forwarded to the remote daemon and returns the same result as calling the
    remote daemon directly.
14. The relay daemon shuts down gracefully after `idle_timeout_secs` of no RPC activity.

### tddy-tools dynamic MCP proxy

15. With `TDDY_REMOTE_*` env vars set and a running relay daemon, `tddy-tools --mcp` reports a
    `tools/list` result that includes `approval_prompt`, `github_create_pull_request`, and
    `github_update_pull_request` (static) **plus** exactly the tools returned by `ListExecTools` —
    no extra tools, no hardcoded remote-tool names.
16. The tddy-tools-side exec-tool catalog (`exec_tool_catalog()`) is a static list mirroring the
    daemon's `tool_catalog()`; a test (`exec_tool_catalog_names_match_workspace_exec_tool_names`)
    guards against the two drifting apart, but a daemon-side catalog rename requires a matching
    manual update in `tddy-tools` — there is no live catalog fetch over either transport
    (`SandboxIpc` has no such message type; it was deliberately scoped out for `DaemonHttp` too).
17. A `call_tool` request for `approval_prompt`, `github_create_pull_request`, or
    `github_update_pull_request` is handled locally (no relay call made).
18. A `call_tool` request for a dynamically-discovered tool name is forwarded to the relay via
    `ExecuteTool` and the `result_json` is returned as the tool result.
19. If `TDDY_REMOTE_*` env vars are not set, `list_tools` returns only the static tools; dynamic
    `call_tool` requests return an error result explaining that remote env is not configured.

### tddy-tools lazy relay lifecycle

20. When `tddy-tools remote start-session` is invoked and no relay daemon is running, the relay daemon
    is started automatically (lazily) and the discovery file `~/.tddy/relay/daemon.json` is created.
21. A second invocation of `tddy-tools remote …` while the relay is already running reuses the same
    relay daemon (same port; no second process started).
22. The relay daemon process survives `tddy-tools` exit; the discovery file remains valid.

### tddy-coder remote mode

23. `tddy-coder --remote --project-id <id> --recipe free-prompting` starts successfully: the relay
    daemon is lazily started, a workspace session is created, context is synced, and the agent is
    invoked.
24. The agent's working directory is a read-only temporary directory. CLAUDE.md (if present in the
    remote repo) is present locally and contains the remote-codebase appendix.
25. The agent's `--allowedTools` list contains `mcp__tddy-tools__<each discovered tool>` and
    `AskUserQuestion`, and **does not contain** `Read`, `Write`, `Edit`, `Glob`, `Grep`, or any bare
    `Bash(...)` pattern.
26. If the agent attempts to call a native file tool (e.g. `Write`), the permission server denies it
    with a message pointing to the `mcp__tddy-tools__*` alternatives.
27. `tddy-coder --remote --session-id <id>` connects to an existing session rather than creating a new
    workspace session.
28. `tddy-coder --remote --resume-from <id>` resumes an existing remote session.

## Local sandbox sibling (darwin, same host)

**Darwin-sandboxed Claude CLI sessions** apply the same *remote codebase* tool model locally:
the agent runs inside a macOS Seatbelt jail and accesses the host git worktree only via
`mcp__tddy-tools__*` calls on a host-initiated gRPC **`SessionChannel`**. There is no LiveKit
relay and no remote daemon — loopback gRPC only, no auth on the sandbox path.

Reuse from this feature area:

- `build_remote_allowlist` / read-only context dir with `REMOTE_APPENDIX`
- `tool_engine::execute_tool` against the host worktree
- Workspace exec tool catalog (`ListExecTools` shapes)

Entry point: `StartSession` with `session_type:"claude-cli"` and `sandbox:true`.
Details: [claude-cli-session.md](claude-cli-session.md#darwin-sandbox-mode-startsessionrequestsandbox--true).

## Non-goals (out of scope)

- Web UI support for remote sessions (no new web screens; operators use the CLI).
- Full semantic code indexing with embeddings for `SemanticSearch` (v1 ships a ripgrep-backed fallback).
- Real LSP diagnostics for `ReadLints` (v1 ships a minimal stub: empty result with a note, or
  `cargo clippy --message-format=json` if a `Cargo.toml` exists).
- Multi-hop forwarding (relay→remote via a chain of intermediate daemons).
- Cross-daemon `ExecuteTool` forwarding from the relay daemon itself (the relay forwards to one named
  remote peer; the remote executes locally).
- Recipes other than `free-prompting` in remote mode (v1 restriction).
- Web-bundle serving from the relay daemon (`--relay` skips `web_bundle_path`).
