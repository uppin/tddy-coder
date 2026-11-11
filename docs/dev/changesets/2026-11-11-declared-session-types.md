# 2026-11-11 — Session types come from YAML declarations; an undeclared one is refused

**Type:** Fix

A project-home `StartSession { session_type: "claude" }` (the spec id, not its session type
`claude-cli`) answered `Permission denied (os error 13)`: an unknown session type fell through into
the tddy-coder tool spawn, which executed an empty `tool_path` resolved to the daemon's working
directory. Session types were strings whose meaning came from their spelling — `workspace`, a `-cli`
suffix naming the agent, and anything else (empty and typos included) a tool session.

## What changed

- **Declared session types.** A tool file declares the session type it runs as in
  `extensions.session { session_type, launch: agent_cli | tddy_coder, label }`
  (`tddy-coding-agent-specs::SessionExtension`, `LaunchKind`). Every PTY built-in declares its
  existing value (`claude-cli`, `codex-cli`, `cursor-cli`, `muse-cli`, `opencode-cli`, `oz-cli`,
  `warp-cli`), so stored sessions resume unchanged; `tool` is declared by
  `session-tools/tddy-coder.yaml` (`builtin_session_tools()`). `CLI_SESSION_TYPE_SUFFIX` is gone.
- **The no-agent session type** `workspace` is the one value nothing declares:
  `tddy-workflow::session_type::{NO_AGENT_SESSION_TYPE, is_no_agent_session_type}`, re-exported by
  `tddy-coding-agent-specs`; a tool file declaring it is refused.
- **Load refusals**: two files declaring one session type (naming both); `agent_cli` on a tool that is
  not a PTY coding agent; an override in `<tddyhome>/tools/` of a built-in with a session section that
  declares none (naming the file, quoting the built-in's values) — which fails the daemon's start.
- **Dispatch on the launch kind.** StartSession and ResumeSession read
  `tddy-agent-launch::harness_from_spec::SessionLaunch { NoAgent, AgentCli, TddyCoder }`
  (`requested_session_launch`, `recorded_session_launch`). An empty or undeclared session type is
  `INVALID_ARGUMENT` naming the value and every offered type; a `tddy_coder` start whose `tool_path` is
  empty or not in `allowed_tools` is `INVALID_ARGUMENT` naming the allowlist
  (`refuse_a_tool_path_the_daemon_does_not_allow`).
- **Records.** tddy-coder records the session type its host declares for the `tddy_coder` launch
  (`declared_tool_session_type` → `tool`). A legacy record naming no session type resumes as a tool
  session when it records a `tool` (`TODO(session-types)`).
- **Clients.** `ListAgentsResponse.session_types` (`SessionTypeInfo { session_type, label, launch }`);
  the StartSession tool schema's `session_type` enum and `DescribeSessionStartOptions.session_types`
  come from the declarations. The web reads them in the `ListAgents` fan-out, keeps one constant
  (`NO_AGENT_SESSION_TYPE`) and decides by the served launch kind; `TOOL_SESSION_TYPE` and
  `isCliSessionType` are gone.
- **Found on the way**: the sandboxed CLI start looked its spec up by session type rather than spec id,
  so a jailed Cursor session got no `.cursor/hooks.json`.

## Docs

- Product: [Session types are declared](../../ft/daemon/session-agent-roster.md#session-types-are-declared);
  [project-concept § Start options](../../ft/daemon/project-concept.md#start-options),
  [claude-cli-session](../../ft/daemon/claude-cli-session.md),
  [cursor-cli-session](../../ft/daemon/cursor-cli-session.md),
  [telegram-session-control](../../ft/daemon/telegram-session-control.md),
  [session-drawer](../../ft/web/session-drawer.md), [session-layout](../../ft/coder/session-layout.md).
- Packages: `tddy-coding-agent-specs` (§ Session types), `tddy-agent-launch` (module layout),
  `tddy-workflow`, `tddy-daemon-rpc`, `tddy-session-activity`, `tddy-session-lifecycle`,
  `tddy-session-create-web`, `tddy-terminal-web`, `tddy-web`.

## Backlog

- Deferred: **"Legacy tool-session records name no session type"**
  (`2026-11-10-tddy-coder-records-no-session-type`) — dropping the legacy-record read rule once no
  record naming no session type needs to resume. Stays open.
- The web follow-up (`2026-11-10-web-session-types-from-the-daemon`) was opened and closed within
  this change; it never reached the backlog on master.
