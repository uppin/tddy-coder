# Tool replacement: no-bash & no-write sessions (`tddy-sandbox-app`)

> **Added: 2026-07-21** — Agent-driven tool restriction for sandboxed CLI sessions (macOS
> in-process path). Everything is declared on the specialized-agent defs themselves
> (`replaces:` — the same array of agent coordinates used for `fastcontext`); there are **no
> mode flags**. Linux/daemon threading is a documented follow-up.

## Purpose

A session's tool surface is shaped entirely by its **agents config** — an array of
`SpecializedAgentDef` coordinates (name, `model`, `base_url` — a local Ollama today; other agent
backends are a def-schema extension, not new code paths). Each def's `replaces:` list may name
**any** exec-catalog tool; every replaced tool is hard-disabled for the main agent and the
binding is **rendered into the in-jail `CLAUDE.md`/`AGENTS.md` appendix** so the agent knows
where each capability went. Two replacements have dedicated surfaces:

**Replacing `Shell` (no-bash).** The def that replaces `Shell` becomes the session's **action
author**. `Shell` and Claude's native `Bash`/`BashOutput`/`KillShell` are hard-disabled; in their
place the main agent gets three session-action tools:

| Tool | Role |
|------|------|
| **`mcp__tddy-tools__request_action`** | Describe a needed command in natural language (`{description, suggested_id?}`). The author subagent writes a [session-action](session-actions.md) YAML manifest for it; after validation it is **auto-established** under `<session_dir>/actions/<id>.yaml` and immediately invocable. Returns `{id, summary, path, has_input_schema}`. |
| **`mcp__tddy-tools__list_actions`** | List established actions (session overlay + per-repo store), same shape as `tddy-tools list-actions`. |
| **`mcp__tddy-tools__invoke_action`** | Invoke an action by id (`{action, data?}`), blocking until the child exits. Returns `{exit_code, stdout, stderr}` (+ `summary` for `result_kind: test_summary`). |

At most one def may replace `Shell` (ambiguous authorship is rejected before spawn).

**Replacing `Write`/`StrReplace`/`Delete` (no-write).** The replacing def is the session's
**coder**: the main agent's mutation tools (plus native `Edit`/`MultiEdit`/`NotebookEdit`)
are hard-disabled and edits are delegated through the existing
`subagent_new_session`/`subagent_prompt` tools. The coder's internal loop gains the
`WRITE`/`STR_REPLACE`/`DELETE` tools (`tddy_discovery::agent_def::SubagentTool`), which its def
**must** bind — a def replacing a mutation tool without the matching internal binding is
rejected before spawn.

Both replacements compose freely with each other and with read-side replacements
(`fastcontext`'s `Grep`/`Glob`/`SemanticSearch`).

## Configuration

Only the agents array (`subagents:` inline in the sandbox config, `<session-base>/agents/*.yaml`
for named defs, or builtins) — see `sandbox-config.example.yaml`:

```yaml
subagents:
  - name: action-author        # replacing Shell ⇒ this agent authors session actions
    model: gemma4:e4b-mlx
    base_url: http://localhost:11434
    replaces: [Shell]
  - name: coder                # replacing the write tools ⇒ this agent is the coder
    model: <stronger local model>
    base_url: http://localhost:11434
    replaces: [Write, StrReplace, Delete]
    tools: [READ, GLOB, GREP, WRITE, STR_REPLACE, DELETE]
```

Per-session opt-in without touching the config: keep the def in the agents dir and activate it
with `--specialized-agent <name>`. Declaring a def inline activates it (and gates spawn on its
warm-up).

## Enforcement layers

Each replaced tool is unreachable at several independent layers:

1. Omitted from `--allowedTools` (`build_claude_allowlist`, `tddy-sandbox-recipes`); a replaced
   `Shell` swaps in the three `ACTION_TOOLS`.
2. Listed in `--disallowedTools` in mcp + native forms, including the differently-named native
   aliases (`native_aliases`: `Bash*` for `Shell`; `Edit`/`MultiEdit`/`NotebookEdit` for
   `Write`) — `build_claude_disallowlist`.
3. Filtered from the in-jail MCP server's advertised catalog (derived from
   `TDDY_SUBAGENTS_JSON`, `PermissionServer::new`); the action tools are merged only when a def
   replaces `Shell` (`shell_replacing_author`).
4. For `Shell`/`Await` only: hard-rejected at the host relay boundary
   (`AppToolHandler::policy_rejects`) — a raw-IPC dispatch from a compromised jail fails too.
   Other replaced tools cannot be host-rejected because the replacing subagent itself dispatches
   them (fastcontext dispatches `Grep`; the coder dispatches `Write`) through the same relay;
   layers 1–3 remain their enforcement.

## Trust model

- **Replacing Shell inserts a gatekeeper, not a privilege reduction.** Established actions
  execute on the host with the same privileges the `Shell` tool had, in the worktree. What
  changes: the main agent can no longer run *arbitrary* strings — only (a) describe intent in
  natural language, and (b) invoke established, schema-validated, fixed-argv manifests. The
  author model is the sole author of `command` vectors; auto-establish makes it the gatekeeper
  (per design decision — no user approval gate).
- **The jail is untrusted; the host re-validates.** The in-jail `request_action` handler
  pre-validates the authored YAML (bounded 3-attempt correction loop, 64 KiB cap) purely as a
  cheap retry mechanism; the authoritative parse / `validate_authored_manifest` (argv non-empty,
  filename-safe id, `input_schema` compiles) / `ensure_action_architecture` / collision check /
  write happen host-side in `host_actions::establish_action`. The write path is fixed to
  `<session_dir>/actions/<id>.yaml`; at invoke time `command` is verbatim from the established
  manifest, `data` is validated against `input_schema`, and `output_path_arg` stays confined by
  `resolve_allowlisted_path`.
- **Idempotence, not redefinition**: re-establishing byte-identical content succeeds; a same-id
  manifest with different content is a collision error — an established action is never silently
  redefined.
- **The coder's Managed-only rule**: the mutation subagent tools work only over
  `CodebaseAccess::Managed`, where path confinement comes from the host tool engine (same as the
  main agent's own writes). `Local` access returns a typed error — a YAML `tools:` entry alone
  must not grant unconfined host writes.

## Implementation map

| Concern | Location |
|---------|----------|
| Allow/disallow lists incl. native aliases + `ACTION_TOOLS` | `packages/tddy-sandbox-recipes/src/claude_cli.rs` (`native_aliases`, `shell_is_replaced`) |
| Replacement validation (one Shell author; coder bindings) | `tddy-sandbox-app::config::validate_tool_replacements` |
| CLAUDE.md/AGENTS.md appendix rendering | `packages/tddy-sandbox/src/context_dir.rs` (`sandbox_remote_appendix`) |
| MCP catalog gating + action tools | `packages/tddy-tools/src/server.rs` (`shell_replacing_author`), `packages/tddy-tools/src/action_tools.rs` |
| Host-side establish/list/invoke + Shell/Await reject | `packages/tddy-sandbox-app/src/{bridge,host_actions}.rs` |
| Shared authored-manifest validation | `tddy_core::session_actions::validate_authored_manifest` |
| Write-capable subagent tools | `packages/tddy-discovery/src/{agent_def,subagent,openai}.rs` |

## Related tests

- `packages/tddy-sandbox-recipes/src/claude_cli.rs` — replaces-driven allow/disallow unit tests
- `packages/tddy-sandbox/src/context_dir.rs` — appendix rendering for a replaced Shell
- `packages/tddy-sandbox-app/src/{config,host_actions}.rs` — replacement validation,
  establish/list/invoke round-trip
- `packages/tddy-tools/tests/request_action_mcp_acceptance.rs` — real `--mcp` wire: defs-driven
  catalog gating, authoring loop with a mocked author model, retry, host-relay
  `EstablishAction` dispatch
- `packages/tddy-tools/src/action_tools.rs` — YAML extraction + pre-validation unit tests
- `packages/tddy-discovery/tests/subagent_write_tools_red.rs` — write-tool dispatch, Local
  rejection, read-only defs never advertise mutation tools

## Follow-ups

1. **Linux/daemon path**: the daemon resolves named defs itself; handle the three action
   dispatches in its `ExecuteTool` path and reuse the same defs-driven derivation.
2. **Backend kinds beyond OpenAI-compatible HTTP** (e.g. a Cursor-driven subagent): extend
   `SpecializedAgentDef` with a backend/kind field — the coordinates array stays the single
   configuration surface.
3. **Async invoke** via `session_action_jobs` (`job_id == task_id`) + `Await`.
4. **Promotion** of session actions to the per-repo catalog (`derive_repo_key`/
   `repo_actions_root` already exist).
5. Runtime guardrails on authored `command[0]` (allow/deny lists) if author-as-sole-gatekeeper
   proves too permissive.

## Related documentation

- [Session actions](session-actions.md) — the manifest/invoke machinery this reuses
- [Specialized subagents](specialized-subagents.md) — the def schema (`replaces`, `tools`)
- `sandbox-config.example.yaml` — worked configuration example
