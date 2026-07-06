# Specialized Subagents (YAML-defined) + Session-Creation Picker

## Summary

Generalizes the single hardcoded FastContext discovery subagent
([discovery-agent.md](discovery-agent.md), [managed-codebase-subagents.md](managed-codebase-subagents.md))
into **specialized subagents**: named, YAML-defined configurations (model, endpoint, system prompt,
bound tools, turn budget) loaded from `<tddyhome>/agents/*.yaml` (`<tddyhome>` = the resolved tddy
data directory, `~/.tddy` in release / `tmp/.tddy` in debug). A `SpecializedAgentDef` is the single
source of truth consumed by three call sites that previously each had their own FastContext-specific
config surface:

1. The **MCP subagent registry** (`tddy-tools`'s `subagent_new_session`/`subagent_prompt`/`subagent_cancel`
   tools) — now backed by any number of registered defs instead of one hardcoded `"fastcontext"` factory.
2. The **standalone `tddy-sandbox-app` CLI** — `--specialized-agent <name>` (repeatable) selects one
   or more defs by name; there is no legacy single-name alias and no override flags.
3. The **workflow backend** (`tddy-coder --agent fastcontext` / `create_backend`) — builds its
   `FastContextBackend` from the resolved def's `model`/`base_url` instead of hardcoded literals.

It also adds the missing UI surface: a collapsible **"Managed codebase"** section in the sandboxed
Claude-CLI session-creation flow that lets a user attach **multiple** specialized subagents to a new
session, closing both non-goals #254 explicitly deferred ("subagents other than FastContext",
"a UI/CLI picker for choosing which subagents to wire").

Zero-config behavior is preserved: without any `<tddyhome>/agents/*.yaml` file, a builtin `fastcontext`
def (identical to today's shipped defaults) is always available.

## Agent definition format

`<tddyhome>/agents/<name>.yaml` (file stem is advisory; `name` inside the file is the registry key):

```yaml
name: my-explorer
label: "My Explorer (local Qwen)"        # optional, defaults to name
model: qwen2.5-coder:7b
base_url: http://localhost:11434         # optional, defaults to http://localhost:30000
system_prompt: |                          # optional; system_prompt_path also supported
  You are a codebase explorer. Answer with <final_answer> citations only.
tools: [READ, GLOB, GREP]                 # optional, defaults to [READ, GLOB, GREP]
max_turns: 10                             # optional, defaults to 10
replaces: [Grep, Glob]                    # optional, defaults to [] (replaces nothing)
```

`replaces` names main-agent exec-catalog tools (e.g. `Grep`, `Glob`, `Read`) this agent takes over —
not the same universe as `tools` above (this agent's own internal READ/GLOB/GREP loop). See
[managed-codebase-subagents.md](managed-codebase-subagents.md) § Tool replacement for the full
enforcement/guidance contract and how multiple agents' `replaces` lists are unioned.

`tools` is an **extensible registry** — `SubagentTool` is a Rust enum with one variant per bound-tool
kind. v1 ships exactly `READ`/`GLOB`/`GREP` (the existing read-only codebase tools); the def
schema/dispatch is generic so a future tool kind is one new variant + one new match arm, not a
schema rework. A def naming an unrecognized tool is rejected at load time (typed error, no silent drop).

Malformed YAML files are skipped with a logged warning at load time — one bad file never prevents the
rest of `<tddyhome>/agents/` (or the builtin fastcontext def) from loading.

## Architecture

```
<tddyhome>/agents/*.yaml  ──scan──►  daemon ListSubagents RPC ──►  Web CreateSessionPane
   (SpecializedAgentDef)             (tddy-discovery::agent_def)   (collapsible + multi-select)
        │                                                                  │
        │ resolve selected defs by name              StartSessionRequest  │ managed_codebase,
        ▼                                                (fields 17-18)   ▼ specialized_agents[]
   daemon start_sandboxed_claude_cli_session
        │  serialize resolved defs → TDDY_SUBAGENTS_JSON env var (jail has no host FS in managed mode)
        │  managed_codebase ⇒ SandboxRunnerSpawn.mounts = [] (repo not mounted)
        ▼
   tddy-tools --mcp (in jail): TDDY_SUBAGENTS_JSON → SubagentRegistry::from_defs(defs)
        │  subagent_new_session { agent: "<name>" } selects the matching def
        ▼
   tddy-discovery::subagent::SpecializedSubagentSession
        (generic multi-turn loop: def.system_prompt seeds history, def.tools gates which
         tool schemas are advertised to the model, def.max_turns bounds the loop)
```

The standalone `tddy-sandbox-app` CLI and the `tddy-coder` workflow backend (`create_backend`) resolve
the same `SpecializedAgentDef` set independently (no daemon in the loop for those two paths) via
`tddy_discovery::agent_def::load_agent_defs(<tddyhome>/agents)` + `builtin_fastcontext_def()`.

## User Story

As a developer, I want to define my own specialized subagent (a different model, endpoint, system
prompt, or a narrower tool set than FastContext's) in a YAML file, and pick which of my defined
subagents — one or several — get wired into a new managed-codebase session from the web UI, instead
of being limited to the single hardcoded FastContext discovery agent and CLI flags.

## Acceptance Criteria

### Agent definitions (`tddy-discovery::agent_def`)

1. `load_agent_defs(dir)` parses every `*.yaml` file in `dir` into a `SpecializedAgentDef`; a
   malformed file is skipped (logged), not a panic and not a silent empty result for the whole dir.
2. `builtin_fastcontext_def()` returns a def with `name: "fastcontext"`,
   `model: "microsoft/FastContext-1.0-4B-RL"`, `base_url: "http://localhost:30000"`,
   `tools: [READ, GLOB, GREP]`, `max_turns: 10` — matching today's shipped defaults exactly.
3. A user-defined YAML file named `fastcontext.yaml` (or any def with `name: fastcontext`) overrides
   the builtin def of the same name — user config always wins over the shipped default.
4. `SubagentTool` deserializes `READ`/`GLOB`/`GREP` (case as shown); an unknown tool name is a typed
   load error, not a silently-dropped tool.

### MCP subagent registry (`tddy-tools`, `tddy-discovery::subagent`)

5. `SubagentRegistry::from_defs(defs)` registers one factory per def; `create(name, access)` returns
   a session built from that def's `model`/`base_url`/`max_turns`/`system_prompt`/`tools`; an unknown
   name returns a typed error (unchanged contract from #254).
6. A session's `system_prompt` (when set) seeds the conversation's first message — today's
   `FastContextSession` starts with no system message at all.
7. A session only advertises its def's bound `tools` to the model — a def binding only `[READ]`
   does not expose GLOB/GREP tool schemas, and a model-issued call to an unbound tool is rejected.
8. A prompt turn that yields no tool call and no `<final_answer>` terminates `EndTurn` with the
   assistant's text as content (today only `<final_answer>` terminates `EndTurn`; a plain-prose
   agent without FastContext's citation convention would otherwise loop until `max_turns`).
9. With `TDDY_SUBAGENTS_JSON` set to a JSON array of defs and `TDDY_SUBAGENT` naming one or more of
   them (comma-separated), `tools/list` over MCP exposes the subagent tools, and
   `subagent_new_session { agent: "<name>" }` selects among the multiple registered defs.
10. Back-compat: `TDDY_SUBAGENT=fastcontext` with no `TDDY_SUBAGENTS_JSON` set still works, resolving
    to `builtin_fastcontext_def()` (today's #254 env-var shape keeps working unmodified).

### Standalone CLI (`tddy-sandbox-app`)

11. `--specialized-agent <name>` is repeatable; each maps to a def resolved from `<tddyhome>/agents`
    + builtins (`spawn::resolve_specialized_agents`), serialized into `TDDY_SUBAGENTS_JSON` alongside
    `TDDY_SUBAGENT` (comma names) via `spawn::subagent_env_overlay`.
12. ~~`--discovery-subagent`/`--fastcontext-url`/`--fastcontext-model`/`--fastcontext-max-turns` keep
    working as deprecated aliases~~ — removed entirely, no backwards compatibility retained.
    `--specialized-agent` (repeatable) is the only way to select an agent, and every agent's
    configuration comes exclusively from its resolved YAML def — see
    [managed-codebase-subagents.md](managed-codebase-subagents.md) AC24.

### Workflow backend (`tddy-coder`)

13. `create_backend("fastcontext", ...)` builds a `FastContextBackend` whose `model`/`base_url` come
    from the resolved `fastcontext` def (builtin or `<tddyhome>/agents/fastcontext.yaml` override),
    not from hardcoded literals.
14. *(partially implemented)* `create_backend` itself accepts any name present in the resolved def
    set. `--agent <name>`'s clap `value_parser` still hardcodes a fixed allowlist and rejects a
    custom name before `create_backend` runs — see the `TODO` at `Args.agent` in
    `packages/tddy-coder/src/run.rs` and `docs/dev/TODO.md`.
15. The def's `system_prompt` does **not** override a workflow goal's own system prompt — per-goal
    system prompts (`GoalHints`, `InvokeRequest.system_prompt`) remain the source of truth for the
    one-shot `CodingBackend::invoke` path; `system_prompt` on a def only seeds the stateful MCP
    subagent conversation (criterion 6).

### Daemon + Web UI

16. `ListSubagents` RPC returns the resolved def set (builtin + `<tddyhome>/agents`) as
    `{name, label, model}` rows.
17. `StartSessionRequest.managed_codebase` (bool) and `.specialized_agents` (repeated string, subagent
    names) are accepted for `session_type == "claude-cli"` **or** `"cursor-cli"` sandboxed sessions; an unknown name in
    `specialized_agents` is a request error, not a silently-dropped agent.
18. When `specialized_agents` is non-empty, the spawned jail's env includes `TDDY_SUBAGENT` +
    `TDDY_SUBAGENTS_JSON` for the resolved agents. (The daemon's sandboxed claude-cli **and** cursor-cli paths already
    never mount the repo — `SandboxRunnerSpawn.mounts` is unconditionally empty there, matching
    today's proxied-tools-only design; `managed_codebase` does not toggle mount behavior on this
    path, unlike `tddy-sandbox-app`'s standalone `--codebase-mode mounted|managed` CLI flag.)
19. The web new-session form shows a collapsible **"Managed codebase"** section when
    `sessionType === "claude-cli"` **or** `"cursor-cli"`; expanding it lists available subagents (from `ListSubagents`) as a
    multi-select; toggling any on sets `managed_codebase: true` and includes the selected names in
    the `StartSessionRequest`.

## Non-goals (out of scope for v1)

- Tool kinds beyond `READ`/`GLOB`/`GREP` (the registry/schema is extensible; only these three ship).
- Streaming partial subagent output mid-turn (inherited from #254).
- Per-project (repo-local) agent defs; hot-reloading `<tddyhome>/agents` while a daemon is running.
- A "Managed codebase" picker for non-sandboxed / plain "tool" session types.
- Live catalog fetch of subagent tool schemas over the transport (inherited from #254).
