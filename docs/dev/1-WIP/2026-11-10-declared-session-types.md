# Changeset: declared-session-types

Fixes the project-home `StartSession { session_type: "claude" }` that answered
`Permission denied (os error 13)`: an unknown session type fell through into the tddy-coder tool
spawn, which exec'd an empty `tool_path` resolved to the daemon's cwd (`~/.tddy`, a directory).

## State A

- A session type was a string whose meaning came from its spelling: `"workspace"` → worktree with
  no agent, `ends_with("-cli")` → the CLI agent `<id>` (`CLI_SESSION_TYPE_SUFFIX`, `strip_suffix`),
  **anything else** (including empty and typos such as `claude`) → a tddy-coder tool session.
- `tool_path` was passed to `resolve_tool_path` unvalidated; empty resolved to the daemon's cwd.
- `"tool"` / `"workspace"` literals were compared in ~20 places across 12 crates and the web.
- `StartSession`'s schema hard-coded `session_type: ["tool", "workspace", …]`; its `tool_path`
  description said "empty is the daemon's default" (there was no such default in the start path).
- `DescribeSessionStartOptions` listed no session types; `provider_harness` / `models_agent`
  matched `"tool"` / `"workspace"`.

## State B

- **Session types come only from declarations.** A tool file declares its session type in a new
  `extensions.session` section (owned by `tddy-coding-agent-specs::SessionExtension`):

  ```yaml
  extensions:
    session: { session_type: claude-cli, launch: agent_cli, label: Claude Code }
  ```

  Every PTY built-in spec declares its existing value (`claude-cli`, `codex-cli`, `cursor-cli`,
  `muse-cli`, `opencode-cli`, `oz-cli`, `warp-cli`), so stored sessions resume unchanged. ACP-only
  specs (`claude-acp`, `codex-acp`) declare none, as before.
- **`tool` is declared by `session-tools/tddy-coder.yaml`** (apart from `specs/`, which holds only coding agents) — a tool file with no `coding_agent` section and
  `session: { session_type: tool, launch: tddy_coder, label: Tool }`. It is embedded through
  `builtin_session_tools()` (kept apart from `builtin_specs()`, every one of which is a coding
  agent), and is present even on a host with no coding agents (`no_coding_agents()`).
- **Launch kinds** are a closed, code-implemented enum, `tddy_coding_agent_specs::LaunchKind`:
  `agent_cli` (the spec-driven PTY launch; refused at load for a tool that is not a PTY coding
  agent) and `tddy_coder`. StartSession and ResumeSession dispatch on
  `tddy_agent_launch::harness_from_spec::SessionLaunch` (`NoAgent` / `AgentCli` / `TddyCoder`),
  never on the session type's spelling.
- **The registry** (`CodingAgentRegistry`) holds the declared session types: `session_type()`,
  `session_types()`, `session_type_of(agent_id)`, `sole_session_type_launching(kind)`, and
  `for_session_type` now looks the declaration up. Loading refuses two files declaring one session
  type (naming both) and a file declaring the reserved no-agent word. `CLI_SESSION_TYPE_SUFFIX` is
  gone.
- **`workspace` is a session with 0 agents**, not a declared type: `NO_AGENT_SESSION_TYPE` and
  `is_no_agent_session_type` live in `tddy-workflow::session_type` (the lowest crate every reader
  depends on), re-exported by `tddy-coding-agent-specs`. Every check goes through the predicate.
- **Validation before anything is created** (`requested_session_launch`, called from
  `start_session_on_request` after routing, from `start_here_body` for every local start, and from
  `refuse_a_start_before_it_begins`): an empty `session_type` and a value that is neither the
  no-agent word nor declared are `INVALID_ARGUMENT`, naming the refused value (`{:?}`) and every
  declared type plus `workspace`.
- **`tool_path`**: a `tddy_coder` start whose `tool_path` is empty or not in `allowed_tools` is
  `INVALID_ARGUMENT` naming the allowlist (`refuse_a_tool_path_the_daemon_does_not_allow`). No
  default is picked.
- **Tool clients see the declared values.** `StartChoices.session_types` (declared types + labels) feed
  the schema's `session_type` enum, plus `workspace`; the choices-less schema leaves it a free string
  pointing at `DescribeSessionStartOptions`, whose answer gains `session_types` (id + label).
  `provider_harness` / `models_agent` tell a coding agent's session type from the injected agent
  choices, and the no-agent type through the predicate.
- **Overrides keep their session type.** `load_agent_registry` refuses an override in
  `<tddyhome>/tools/` of a built-in that declares `extensions.session` when the override declares
  none, naming the override file and quoting the built-in's `{ session_type, launch, label }`.
  Nothing is inherited. A new tool file that overrides no built-in may declare none. The daemon's
  start (`runtime::build` → `host_coding_agents`) fails with that message, so the operator sees it
  as a startup error (`tddy-daemon/tests/host_registry_load.rs`).
- **Records name their session type.** tddy-coder now writes the session type its host declares
  for the `tddy_coder` launch (`declared_tool_session_type`, read from the registry — `tool`) into
  every `.session.yaml` it starts. Stored `claude-cli` / `workspace` / `tool` records resume as
  before; a **legacy** record with no session type resumes as `tddy_coder` when it records a
  `tool`, and is refused otherwise (`recorded_session_launch`, `TODO(session-types)`).
- **Web reads the declarations.** `ListAgentsResponse.session_types` (`SessionTypeInfo { session_type,
  label, launch }`) is served by the daemon and read in the same fan-out as the agents
  (`useSelectableAgents`). The bundle holds one session-type constant, `NO_AGENT_SESSION_TYPE`
  (`cliHarnessOptions.ts`, mirroring the Rust one); every other decision is the served launch kind
  (`sessionLaunchOf`, `knownSessionLaunch`): the type toggle (tddy-coder types first, defaulting to
  the first), the request builder, the Create-Remote-Branch toggle, the model/provider harness, the
  project-home main gate, the terminal "+" menu and tmux listing, the drawer's mobile shortcuts and
  the attach rules (where a legacy record's empty type still reads as a tool session).
  `TOOL_SESSION_TYPE` and `isCliSessionType` are gone.
- **Found on the way**: the sandboxed CLI start looked its spec up by session type
  (`agents().get(kind.name)`), so a jailed Cursor session got no `.cursor/hooks.json`; it now looks
  up `kind.spec_id`.

## Boundaries

- `tddy-actions`' action-kind catalogue (`"claude-cli"` as an action kind), the removed-key
  migration message in `tddy-daemon-kernel::config::removed_agent_keys`, and the `workspace`
  branch prefix in `tddy-worktree-service::branch_intent` are not session-type dispatch and are
  left as they are.
- Telegram's workflow spawn starts tddy-coder directly (not through `StartSession`) with
  `default_tool_path()`; unchanged.

## TODO

- [x] Product docs: `docs/ft/daemon/session-agent-roster.md` § Session types are declared
- [x] Changeset — this document
- [x] Implement production code (daemon, tddy-coder, web)
- [x] Web: session types and launch kinds from the daemon
- [x] tddy-coder records its declared session type

Deferred, outside this changeset's scope: dropping the legacy-record read rule once no record
naming no session type needs to resume — [`2026-11-10-tddy-coder-records-no-session-type.md`](../todo/2026-11-10-tddy-coder-records-no-session-type.md)
