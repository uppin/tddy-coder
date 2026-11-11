# 2026-11-11 — Session types come from YAML declarations

**Type:** Fix

- `cliHarnessOptions.ts`: session types come from `ListAgentsResponse.session_types` (`ServedSessionType`, `sessionTypeOptions` — `tddy_coder` launches first), decisions read the served launch kind (`sessionLaunchOf`, `knownSessionLaunch`, `AGENT_CLI_LAUNCH`, `TDDY_CODER_LAUNCH`); the one session-type constant is `NO_AGENT_SESSION_TYPE`. `TOOL_SESSION_TYPE` and `isCliSessionType` removed.
- `useSelectableAgents` returns `sessionTypes` from the same `ListAgents` call. The type toggle opens on the first served type; the request builder, Create-remote-branch toggle, model/provider harness and project-home main gate read `sessionLaunch`. The chosen session type is always sent.
- Docs: [create-session-provider](../create-session-provider.md).

Cross-package: [declared session types](../../../../docs/dev/changesets/2026-11-11-declared-session-types.md). Product: [Session types are declared](../../../../docs/ft/daemon/session-agent-roster.md#session-types-are-declared).
