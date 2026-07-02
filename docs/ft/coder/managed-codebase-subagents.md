# Managed-Codebase Mode + Discovery Subagents (ACP-shaped MCP)

## Summary

Formalizes today's "remote codebase" concept as a named session mode — **managed codebase** —
and adds the ability to wire **specialized subagents** into that mode. A subagent is a
conversational helper (starting with the existing FastContext discovery agent,
[discovery-agent.md](discovery-agent.md)) that the main coding agent (Claude Code) can open a
**conversation thread** to, over MCP, and ping-pong codebase questions against — instead of doing
every exploration step itself.

Managed-codebase mode is the *existing* behavior, renamed for users: the real codebase lives on a
daemon/host the agent cannot touch directly; every file/shell operation is proxied through
`mcp__tddy-tools__*` tools (see [remote-codebase-mode.md](../daemon/remote-codebase-mode.md)). This
feature does not change that proxying — it adds a second, independent MCP surface on the same
`tddy-tools --mcp` server: a small set of **subagent tools** shaped after **ACP**
(Agent Client Protocol) terminology, so opening/continuing a subagent conversation feels like the
same `session/new` → `session/prompt` shape the codebase already uses for `ClaudeAcpBackend` /
`CodexAcpBackend` ([codex-acp-backend.md](codex-acp-backend.md)).

## ACP → MCP tool mapping

| ACP concept (`agent-client-protocol` crate)      | MCP tool on `tddy-tools --mcp`                                     |
|---------------------------------------------------|----------------------------------------------------------------------|
| `session/new` (`NewSessionRequest`)               | `subagent_new_session` — input `{ agent?, sessionId?, cwd? }` → `{ sessionId }` |
| Client-chosen `SessionId`                         | `sessionId` input — the **main agent** decides the conversation id; a fresh id is generated only when omitted |
| `session/prompt` (`PromptRequest`)                | `subagent_prompt` — input `{ sessionId, prompt: [ContentBlock] }` |
| `PromptResponse.stopReason`                       | output field `stopReason`: `"end_turn"` \| `"max_turn_requests"` \| `"cancelled"` |
| Response `content` (`ContentBlock[]`)             | output field `content`: `[{ "type": "text", "text": "..." }]` |
| `session/cancel`                                  | `subagent_cancel` — input `{ sessionId }` |

These tools use plain JSON (serde), not the `agent-client-protocol` crate — that crate's
`Client`/`Agent` JSON-RPC machinery drives a *subprocess*; here the "subagent" runs as an in-process
loop inside `tddy-tools`, exposed as ordinary MCP tools. Only the vocabulary (`sessionId`,
`stopReason`, `end_turn`, `ContentBlock`) is mirrored, so a reader familiar with ACP recognizes the
shape immediately.

"Yield when there's an opportunity for an extra prompt" (the requirement that the subagent's
internal tool-call ↔ tool-result loop hands control back to the main agent) = the loop terminating
either on a `<final_answer>` (→ `stopReason: "end_turn"`) or on hitting its configured turn budget
(→ `stopReason: "max_turn_requests"`) — mirroring `FastContextBackend`'s existing termination
conditions (`discovery-agent.md`), just exposed per-turn instead of only at the very end of a whole
invocation.

## Architecture

```
Claude Code (main agent, managed-codebase mode: native FS tools excluded)
   │  MCP (rmcp, stdio)
   ▼
tddy-tools --mcp  (PermissionServer)
   ├── exec tools  Read/Write/Shell/…  ──► dispatch_session_tool ──► daemon ExecuteTool (real worktree)
   └── subagent tools (NEW):
          subagent_new_session / subagent_prompt / subagent_cancel
              │  PermissionServer holds sessionId → Box<dyn SubagentSession>
              ▼
          tddy_discovery::subagent::SubagentRegistry  →  FastContextSession
              │  internal READ/GLOB/GREP tool-call ↔ tool-result loop
              ▼
          tddy_discovery::subagent::CodebaseAccess
              ├── Local    — direct host filesystem (co-located subagent)
              └── Managed  — injected dispatch fn (session_tool_client::dispatch_session_tool)
```

`CodebaseAccess` lets the *same* `FastContextSession` either read the host filesystem directly
(when the subagent runs on the same host as the real worktree — e.g. `tddy-sandbox-app` without
`--remote-codebase`) or read through the exact same proxy the main agent's exec tools use (when the
codebase is only reachable via the daemon). `tddy-discovery` never depends on `tddy-tools`; the
managed dispatch function is injected by the caller (`tddy-tools`) as a boxed async closure, keeping
the dependency direction `tddy-tools → tddy-discovery` (already true today via `FastContextBackend`
in `tddy-coder`) and never the reverse.

## User Story

As a developer running Claude Code in managed-codebase mode, I want to hand codebase-discovery
questions to a lightweight local-model subagent — instead of spending the main agent's own
tool-call budget exploring the repo — and keep talking to that same subagent across multiple
questions in one session, so that discovery stays fast and cheap without giving the main agent
direct filesystem access.

## Acceptance Criteria

### Subagent session lifecycle (`tddy-discovery`)

1. `SubagentRegistry::create("fastcontext", config)` returns a `Box<dyn SubagentSession>`; an
   unknown name returns a typed error, not a panic or a silent default.
2. A `FastContextSession` retains its message history across multiple `prompt()` calls — a second
   `prompt()` sees the model's and tool results from the first, matching a real multi-turn
   conversation rather than resetting each call.
3. `prompt()` returns `stop_reason: EndTurn` when the model produces a `<final_answer>`, and
   `stop_reason: MaxTurnRequests` when the configured per-prompt turn budget is exhausted with no
   `<final_answer>` — never panics, never loops forever.
4. `CodebaseAccess::Local` executes READ/GLOB/GREP against the local filesystem (same semantics as
   `ToolExecutor::Local`).
5. `CodebaseAccess::Managed` maps READ/GLOB/GREP to `Read`/`Glob`/`Grep` and dispatches them through
   an injected async function rather than `ToolExecutor::Remote`'s own HTTP client — the same
   function `tddy-tools` already uses for its exec-tool proxying
   (`session_tool_client::dispatch_session_tool`), so a managed subagent and the main agent's exec
   tools share one transport-detection path.

### MCP surface (`tddy-tools`)

6. With `TDDY_SUBAGENT=fastcontext` set, `tools/list` over the real MCP stdio wire includes
   `subagent_new_session`, `subagent_prompt`, and `subagent_cancel`; without it, none of the three
   are present.
7. `subagent_new_session` with a caller-supplied `sessionId` uses that exact id — the main agent, not
   the subagent server, decides the conversation id (matching the plan's "main agent decides the
   conversation ID" requirement).
8. `subagent_prompt` against a `sessionId` opened by `subagent_new_session` returns
   `{ stopReason, content }`; a second `subagent_prompt` call against the same `sessionId` continues
   the same conversation (criterion 2, exercised end-to-end over the MCP wire).
9. `subagent_prompt` against an unknown `sessionId` returns an error result (`is_error:true`), not a
   panic and not a silently-created new session.

### Allowlist (`tddy-sandbox-recipes`)

10. The sandboxed Claude CLI `--allowedTools` list includes
    `mcp__tddy-tools__subagent_new_session`, `mcp__tddy-tools__subagent_prompt`, and
    `mcp__tddy-tools__subagent_cancel` when a discovery subagent is enabled for the session, and
    omits all three when it is not.

### `tddy-sandbox-app` spawn wiring

11. `--codebase-mode managed` is accepted and is equivalent to today's `--remote-codebase`; the
    latter remains a working (deprecated) alias.
12. `--discovery-subagent fastcontext` (with `--fastcontext-url`/`--fastcontext-model`/
    `--fastcontext-max-turns`) is threaded into the spawned sandbox's environment so the in-jail
    `tddy-tools --mcp` process constructs a `fastcontext` subagent on demand.

## Non-goals (out of scope for v1)

- Live catalog fetch of subagent tool schemas over the transport (mirrors the existing
  `exec_tool_catalog()` limitation — see remote-codebase-mode.md AC16).
- Streaming partial subagent output back to the main agent mid-turn (a `subagent_prompt` call
  returns only once the subagent yields).
- ~~Subagents other than FastContext~~ — addressed by
  [specialized-subagents.md](specialized-subagents.md): the registry now resolves any number of
  YAML-defined agents (`<tddyhome>/agents/*.yaml`), not just the hardcoded `"fastcontext"` factory.
- Renaming the internal `RemoteToolEnv` / `TDDY_REMOTE_*` wire vocabulary to "managed" — only the
  user-facing surface (CLI flags, help text, context-dir appendix prose, docs) is renamed; the
  daemon/sandbox IPC wire and its tests are left alone.
- ~~A UI/CLI picker for choosing which subagents to wire~~ — addressed by
  [specialized-subagents.md](specialized-subagents.md) for the daemon-driven web UI (a collapsible
  "Managed codebase" multi-select in session creation); the standalone `tddy-sandbox-app` CLI
  picker remains flag-driven only (tracked in `docs/dev/TODO.md`).
