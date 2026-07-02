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

## Tool replacement (subagent-declared)

Wiring in a subagent is additive-only today: the three `subagent_*` tools are added, but the main
agent keeps its full exec-tool set, so nothing steers it toward actually using the subagent instead
of grepping/globbing the codebase itself.

A subagent can declare the exec tools it **replaces** (FastContext replaces `Grep`/`Glob` — its own
internal READ/GLOB/GREP loop already covers that ground). When a subagent with a non-empty replaced
set is wired in:

- **Enforcement (hard):** the replaced tools are dropped from the sandboxed Claude CLI's
  `--allowedTools` before the `mcp__tddy-tools__` prefix is applied — a direct call to a replaced
  tool is impossible, not merely discouraged.
- **Guidance (soft):** the managed-codebase appendix in CLAUDE.md/AGENTS.md is rendered to say those
  tools are unavailable and name the subagent that must be used instead.

The declared set has a per-subagent default (`tddy_discovery::subagent_replaced_tools`), carried as
`TDDY_SUBAGENT_REPLACES` into the jail. There is no caller-facing override for it (nor for a
subagent's `model`/`base_url`/`max_turns`) — all of it comes exclusively from the resolved agent's
YAML def (or the builtin `fastcontext` def); the earlier `--fastcontext-url`/`--fastcontext-model`/
`--fastcontext-max-turns`/`--subagent-replaces` flags and their `StartSessionRequest`/
`SessionMetadata` equivalents were removed (see criterion 24).

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
12. `--specialized-agent fastcontext` is threaded into the spawned sandbox's environment so the
    in-jail `tddy-tools --mcp` process constructs a `fastcontext` subagent on demand, using that
    def's own YAML-declared `base_url`/`model`/`max_turns` (see criterion 24 — there is no
    caller-facing override).

### Tool replacement (`tddy-discovery`, `tddy-sandbox`, `tddy-sandbox-recipes`, `tddy-sandbox-app`)

13. `tddy_discovery::subagent_replaced_tools("fastcontext")` returns `["Grep", "Glob"]`; an unknown
    subagent name returns an empty set (no panic, no fabricated tool name).
14. `tddy_discovery::resolve_replaced_tools(name, override_csv)` returns the declared default when
    `override_csv` is `None` or empty, and the override's tool names (normalized to the exec
    catalog's casing) when non-empty — the override always wins over the default, never merges with
    it. A token that doesn't match a known exec tool is dropped rather than passed through.
15. `tddy_sandbox_recipes::build_claude_allowlist(subagent_enabled, replaced)` omits
    `mcp__tddy-tools__<Tool>` for every `Tool` in `replaced`, while every other exec tool from
    `tddy_sandbox::workspace_exec_tool_names()` (plus `AskUserQuestion`, plus the subagent tools when
    `subagent_enabled`) is still present. An empty `replaced` slice reproduces today's full-exec
    allowlist exactly (no regression for sessions without a replacing subagent).
16. `tddy_sandbox::context_dir::sandbox_remote_appendix(subagent, replaced)` — when `replaced` is
    non-empty — states that those tools are not available as direct tools and names the subagent
    that must be used for them, in addition to (not instead of) listing the still-available exec
    tools. When `replaced` is empty, the rendered text is unchanged from today's appendix.
17. ~~`tddy-sandbox-app`'s `subagent_env_overlay` sets `TDDY_SUBAGENT_REPLACES` only when an
    explicit override is given~~ — superseded by criterion 24: there is no override anymore.
    `TDDY_SUBAGENT_REPLACES` always carries the resolved def's own declared `replaces`
    (normalized), set unconditionally whenever exactly one agent is wired.
18. ~~`tddy-daemon`'s own sandboxed-session path threads `StartSessionRequest`'s
    `fastcontext_url`/`fastcontext_model`/`fastcontext_max_turns`/`subagent_replaces` fields~~ —
    those fields were removed (criterion 24), along with the `discovery_subagent` field itself
    (criterion 24) and its parallel `TDDY_SUBAGENT_FASTCONTEXT_*` env mechanism. `specialized_agents`
    (even a single-element array) is the only wiring path, for both new-session start and resume.

### Tool replacement, generalized to the specialized-agent array

[specialized-subagents.md](specialized-subagents.md) generalized subagent wiring from one hardcoded
name to an array of YAML-defined `SpecializedAgentDef`s, but shipped with no tool-replacement
wiring for that array path — only the single-name `discovery_subagent` path (criteria 13-18 above,
since removed) enforced/rendered replaced tools. The following criteria connect the two: every
specialized agent in the array can declare its own replaced-tool set, and `tddy-sandbox-app` is
fully migrated onto the array model.

19. `SpecializedAgentDef` gains a `replaces: Vec<String>` field (`#[serde(default)]` — absent in
    YAML defaults to `[]`, replacing nothing). `builtin_fastcontext_def()` sets
    `replaces: ["Grep", "Glob"]`, matching criterion 13's single-name default exactly (single
    source of truth — `tddy_discovery::subagent::subagent_replaced_tools("fastcontext")` now derives
    from this field rather than a separate hardcoded literal).
20. `tddy_discovery::subagent::normalize_replaced_tools(tokens)` trims, case-insensitively matches
    against the canonical exec-tool catalog, canonicalizes casing, drops unrecognized tokens, and
    de-duplicates preserving first-occurrence order.
    `resolve_replaced_tools_for_defs(defs: &[SpecializedAgentDef])` unions every def's own
    `replaces` list through that same normalization — the array-model counterpart to criterion 14's
    single-name `resolve_replaced_tools`.
21. `tddy_sandbox::context_dir::sandbox_remote_appendix(replacements: &[SubagentReplacement])`
    accepts one entry per active agent (`SubagentReplacement { name, replaced }`) and renders a
    per-agent breakdown — each agent named next to the tools it specifically replaces, not a single
    flattened list — plus an "pass `agent: "<name>"` to select which subagent" hint when more than
    one agent is active. An empty `replacements` slice (or one where every entry's `replaced` is
    empty) reproduces today's unchanged appendix (no regression for sessions without a replacing
    subagent). `SandboxContextDir::create_with_subagent` takes the same `&[SubagentReplacement]`
    slice, replacing its old single `(subagent: Option<&str>, replaced: &[&str])` signature.
22. `tddy-daemon` has a single wiring path: `specialized_agents` (array — even a single-element
    array selects exactly one agent). `ConnectionServiceImpl::resolve_specialized_agent_defs`
    resolves `specialized_agents` names once per call (unknown name ⇒ `InvalidArgument`, naming the
    unresolvable agent); `specialized_subagent_env` builds the `TDDY_SUBAGENT`/`TDDY_SUBAGENTS_JSON`
    env pair from the already-resolved defs. The per-agent `SubagentReplacement` list feeds
    `prepare_context_dir_with_subagent` for both `start_sandboxed_claude_cli_session` and
    `relaunch_sandboxed_runner` (the latter takes a `specialized_agents: &[String]` parameter, so a
    resumed session re-resolves and re-wires the same defs it started with). There is no
    `discovery_subagent` field, and therefore no mutual-exclusivity concern to guard against.
23. `SessionMetadata` has a `specialized_agents: Vec<String>` field
    (`#[serde(default, skip_serializing_if = "Vec::is_empty")]`) — omitted from `.session.yaml` when
    empty, defaults to empty for legacy files without the key. `resume_sandboxed_claude_cli_session`
    reads `meta.specialized_agents` and passes it straight through to `relaunch_sandboxed_runner`
    (criterion 22), so a resumed session's specialized-agent wiring survives a daemon restart. There
    is no `discovery_subagent` field on `SessionMetadata` to fold in.
24. `tddy-sandbox-app` is migrated off the single-subagent-only flag set onto the array model:
    `--specialized-agent <name>` (repeatable) + `--agents-dir` (default `<session-base>/agents`) is
    the only way to wire a subagent in — **no backwards compatibility was retained**.
    `--discovery-subagent` (the deprecated single-name alias) was removed entirely, not merely
    deprecated: from `tddy-sandbox-app`'s CLI, from `StartSessionRequest`'s proto field 19, from
    `SessionMetadata`, and from `tddy-daemon`'s request handling — a caller wanting exactly one agent
    passes a single-element `specialized_agents`/`--specialized-agent` value. **The legacy
    `--fastcontext-url`/`--fastcontext-model`/`--fastcontext-max-turns`/`--subagent-replaces`
    override flags were also removed entirely** (from `tddy-sandbox-app`'s CLI and
    `SubagentSpawnConfig`, from `StartSessionRequest`'s proto fields 20-23 — now `reserved` — and
    daemon threading, and from `SessionMetadata`): every specialized agent's configuration comes
    exclusively from its resolved YAML def (or the builtin `fastcontext` def), with no
    caller-facing override at any layer. `spawn::subagent_env_overlay(defs)` emits `TDDY_SUBAGENT`
    (comma names) + `TDDY_SUBAGENTS_JSON` (serialized defs) for any number of agents, plus
    `TDDY_SUBAGENT_REPLACES` in the single-agent case (always the def's own declared `replaces`) —
    the same env shape `tddy-sandbox-runner` and `tddy-tools --mcp` already consume via
    `TDDY_SUBAGENTS_JSON` (criterion 9). This closes out `docs/ft/coder/specialized-subagents.md`
    ACs 11-12 (previously tracked as unimplemented in `docs/dev/TODO.md`).

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
- Extending tool-replacement enforcement to the `tddy-coder --remote` path (that path does not wire
  subagents at all today — see `docs/dev/TODO.md`).
- Per-tool replacement policies beyond a flat replaced-set (e.g. partial replacement of `Grep` for
  some file types only).
