# Project-home agent reaches its tools - PRD

**Date**: 2026-10-24
**PRD Type**: Bug Fix

## Affected Features

- **Primary Feature**: [Project concept](../project-concept.md) — § Project-home session and §
  Project-home tools: the co-located claude-cli main is launched wired to `tddy-tools`, with its
  native tools withdrawn and a role prompt.
- **Related Feature**: [Claude CLI session](../claude-cli-session.md) — a main's launch argv and
  environment differ from an ordinary session's (MCP config, disallowed native tools, appended
  system prompt, `TDDY_REMOTE_*`).
- **Related Feature**: [Remote codebase mode](../remote-codebase-mode.md) — the `TDDY_REMOTE_*`
  transport (daemon unix socket or HTTP) gains a third producer: the co-located main.

## Summary

A project's main (the `project-home` session) is started with its tool set pre-approved but with no
`tddy-tools` MCP server behind it, so none of its tools exist. It cannot list, start, read or diff
the project's sessions, and it keeps Claude's native Bash and file tools, so it is not read-only.
This change launches (and resumes) a claude-cli main with the `tddy-tools` MCP server as its only
MCP server, wired to this daemon with a credential of its own, with every native filesystem and
shell tool withdrawn, and with a short prompt telling it what it is.

## Background

On Tddy Desktop, session `01a16dc6-ab4c-7dc3-844e-baca4514f86e` (main of the tddy-coder project) was
asked to start a session. Its `claude` had `--allowedTools mcp__tddy-tools__StartProjectSession`
(and the other project-home tools) but no `--mcp-config` and no `TDDY_REMOTE_*` variables, so the
tools never appeared. It reached for native Bash, tried `tddy-tools spawn-conversation` (managed
workflows only; `TDDY_SOCKET not set`), and asked the operator what they meant.

`#project-home` 5/7 made the daemon serve the tools (`ExecuteTool`, `ListExecTools`) and pre-approved
them on the agent, but nothing in the unsandboxed claude-cli launch registers the server that would
call them. The PRD's read-only guarantee holds on the `ExecuteTool` route only; native tools never
cross it.

## Proposed Changes

### What's Changing

- **MCP server.** A claude-cli main is launched with `--mcp-config <session dir>/claude-mcp-config.json`
  registering `tddy-tools --mcp` (the deployment's toolchain binary, absolute path), and with
  `--strict-mcp-config`, so it is the only MCP server the main loads.
- **Transport.** The main's process environment carries `TDDY_REMOTE_SESSION_ID` = the main's own
  session id and `TDDY_REMOTE_SESSION_TOKEN` = a credential minted for the agent, plus:
  - on an **embedded** daemon (Tddy Desktop): `TDDY_REMOTE_DAEMON_SOCKET` = the daemon's agent-tool
    socket, which `tddy-tools` prefers;
  - on a **binary** daemon (`tddy-daemon`): `TDDY_REMOTE_DAEMON_URL` = this daemon's local HTTP URL,
    and no socket.
- **Credential.** The agent's token is minted by this daemon under the **verified** caller of the
  ensure (or resume), with the split agent's lifetime (24 h). The caller's own token is never
  forwarded. A caller token that does not verify refuses the start (or resume) with
  `UNAUTHENTICATED`, naming the main, and starts nothing.
- **Native tools withdrawn.** The main is launched with `--disallowedTools` for every native
  filesystem and shell tool (`Read`, `Write`, `Edit`, `MultiEdit`, `NotebookEdit`, `Bash`,
  `BashOutput`, `KillShell`, `Glob`, `Grep`, `LS`). Its only reach is the `mcp__tddy-tools__*` set
  the daemon admits and enforces.
- **Role prompt.** The main is launched with `--append-system-prompt-file <session dir>/<file>`
  stating that it is the project's main session on a read-only home worktree, that it has no shell
  and cannot edit files, and naming the tools it uses to see and start the project's sessions and to
  inspect and manage their worktrees (`ListProjectSessions`, `StartProjectSession`,
  `ReadSessionWorktree`, `DiffSessionWorktree`, and the session git tools).
- **Resume.** A resumed main (ensure on a stopped main, or `ResumeSession`) is wired the same way,
  with a freshly minted token.

### What's Staying the Same

- The project-home tool set, its `--allowedTools` pre-approval, and the daemon's enforcement
  (`ExecuteTool` role guard, `ListExecTools` answer) are unchanged.
- Ordinary claude-cli sessions get none of this; their argv and environment are unchanged.
- The sandboxed main stays refused
  ([TODO](../../../dev/todo/2026-10-24-sandboxed-main-needs-session-role.md)).
- Hooks keep reporting as they do today (agent-tool socket on an embedded host, HTTP otherwise).
- `--permission-mode` is whatever the start spec says; no `--permission-prompt-tool` is added.

## Impact Analysis

### Technical Impact

- `tddy-agent-launch`: the role's launch input grows from an `--allowedTools` list to argv plus
  environment, at start and resume; the start path gains the caller's token for minting.
- `tddy-session-split`: the agent-token mint and the co-located env builder are reached from
  `tddy-agent-launch` (visibility widened or a `pub` wrapper; mint refusal wording no longer says
  "split session").
- No new crate dependency, no proto change, no new RPC.

### User Impact

- A main can actually start, list, read, diff and manage the project's sessions from its chat.
- A main can no longer run shell commands or edit files in its home worktree. Anything it needs to
  do on code it does by starting a session.
- A stopped main resumed after this lands gets the new wiring; no migration.

## Implementation Plan

1. Build the main's launch wiring (MCP config, `TDDY_REMOTE_*` env for the host kind, native-tool
   disallow list, prompt file) in one function in `tddy-agent-launch`.
2. Call it at start (minting the token from the caller's) and at resume.
3. Acceptance tests over `EnsureProjectHomeSession` with a stub `claude` that echoes argv and env;
   unit tests for the embedded/binary host split.

## Acceptance Criteria

- [ ] A started main loads `tddy-tools --mcp` as its only MCP server ([Project concept](../project-concept.md))
- [ ] A started main's tools reach this daemon as the main: its own session id, a token minted for it that verifies as the caller ([Project concept](../project-concept.md))
- [ ] On an embedded daemon the main reaches it over the agent-tool socket; on a binary daemon over its HTTP URL ([Remote codebase mode](../remote-codebase-mode.md))
- [ ] A started main has no native filesystem or shell tool ([Project concept](../project-concept.md))
- [ ] A started main is told it is the project's main and which tools to use ([Project concept](../project-concept.md))
- [ ] A resumed main is wired the same way, with a fresh token ([Project concept](../project-concept.md))
- [ ] An ordinary claude-cli session's launch is unchanged ([Claude CLI session](../claude-cli-session.md))
- [ ] A main whose caller token does not verify is refused and starts nothing (already true: the ensure's user resolver refuses it before any mint; the mint's own refusal is a second, unreachable-in-production layer)

## References

### Affected Features (Complete List)

- [Project concept](../project-concept.md) — main's launch wiring, read-only now enforced on native tools too
- [Claude CLI session](../claude-cli-session.md) — role-specific argv/env
- [Remote codebase mode](../remote-codebase-mode.md) — co-located main as a `TDDY_REMOTE_*` producer

### Related Documentation

- Changeset: [2026-10-24-project-home-agent-tools.md](../../../dev/1-WIP/2026-10-24-project-home-agent-tools.md)
