# Changeset: Agent-driven tool replacement (no-bash & no-write sessions)

**PRD**: `docs/ft/coder/no-bash-mode.md`
**Branch**: `no-bash-no-write-modes`

## Checklist

- [x] Create changeset
- [x] tddy-sandbox-recipes: `replaces`-driven builders — `native_aliases` (Bash family for
      Shell; Edit family for Write), `ACTION_TOOLS` merged into the allowlist when `Shell` is
      replaced (`shell_is_replaced`)
- [x] tddy-sandbox-app: `validate_tool_replacements` (single Shell author; mutation replacers
      must bind WRITE/STR_REPLACE/DELETE); replaced set threaded to the host tool handler
- [x] tddy-sandbox: `sandbox_remote_appendix` renders the session-action surface for a replaced
      `Shell` (bindings land in the in-jail CLAUDE.md/AGENTS.md)
- [x] tddy-discovery: `SubagentTool::{Write, StrReplace, Delete}`, Managed-only
      `CodebaseAccess::{write, str_replace, delete}`, `mutation_tool_definitions()` (kept out of
      the unfiltered FastContext tool list)
- [x] tddy-core: shared `session_actions::validate_authored_manifest`
- [x] tddy-tools: defs-driven gating (`shell_replacing_author` from `TDDY_SUBAGENTS_JSON`); new
      `action_tools.rs` (`request_action` author loop with bounded retries, `list_actions`,
      `invoke_action`)
- [x] tddy-sandbox-app: host-side `EstablishAction`/`ListActions`/`InvokeAction` handlers
      (`host_actions.rs`) + `Shell`/`Await` host-boundary reject when Shell is replaced
- [x] Example config + feature doc + session-actions.md cross-reference
- [x] Tests: recipes replaces-driven units; sandbox appendix rendering; app
      validation/host-actions units; tools MCP acceptance
      (`request_action_mcp_acceptance.rs`); discovery write-tools units

## Motivation

A sandboxed CLI session's `Shell` tool is arbitrary host command execution. Rather than adding
mode flags, the session's tool surface is shaped entirely by its **agents config** — the
existing array of `SpecializedAgentDef` coordinates. A def's `replaces:` may name **any** exec
tool; replacing `Shell` makes that def the session's action author (commands become enumerable,
declarative session-action manifests, re-validated host-side), and replacing the mutation tools
makes a def the session's coder. Bindings are rendered into the in-jail CLAUDE.md so the main
agent knows where each capability went — no code changes needed to rebind a tool.

## Packages touched

`tddy-sandbox-recipes`, `tddy-sandbox`, `tddy-sandbox-app`, `tddy-discovery`, `tddy-core`
(one shared validator), `tddy-tools`.

## Follow-ups

- Linux/daemon path: defs-driven derivation + action dispatches in the daemon `ExecuteTool` path
- Backend kinds beyond OpenAI-compatible HTTP (e.g. Cursor) as a `SpecializedAgentDef` extension
- Async `invoke_action` via `session_action_jobs`
- Session→repo-catalog action promotion
