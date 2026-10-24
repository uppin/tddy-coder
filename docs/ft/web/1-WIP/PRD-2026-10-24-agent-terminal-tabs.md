# Agent terminal tabs — any CLI harness beside bash, in the session's worktree — PRD

**Date**: 2026-10-24
**PRD Type**: Feature Enhancement
**Stack**: `#warp-agent` 2/2 — branch `feature/warp-agent/agent-terminal-tabs`, based on `feature/warp-agent/warp-cli-session` (#717), which registers Warp as a CLI harness.

## Affected Features

- **Primary**: [Session terminal tabs](../session-terminal-tabs.md) — the `+` opens a menu: **Bash**
  or any registered agent. Tabs are labelled by what they run.
- [Terminal sessions](../../daemon/terminal-sessions.md) — `StartTerminalSession` gains a `kind`.
  Acceptance criterion 3 ("starts a **Bash tool** … with **no inputs**") becomes "starts the tool
  `kind` names; empty is the login shell". The non-goal "Tools that take inputs. The API leaves room
  for them…" is half-lifted: a kind is a tool choice, not tool inputs.
- [Warp CLI session](../../daemon/warp-cli-session.md), [Muse Code](../../daemon/muse-cli-session.md),
  [OpenCode](../../daemon/opencode-cli-session.md), [Cursor](../../daemon/cursor-cli-session.md),
  [Claude Code](../../daemon/claude-cli-session.md) — each gains one line saying it can also run as an
  agent tab inside another session.

## Summary

A running session's terminal tab strip can open an **agent tab**: a registered CLI harness (Claude
Code, Cursor Agent, OpenCode, Muse Code, Warp) running interactively in the **same worktree** as the
session's main agent, next to it. An operator working in a Claude Code session can open Warp beside
it, or a second Claude, and switch between them like bash tabs. The agents share the checkout and do
not talk to each other. The operator coordinates them, as with bash tabs today.

## Background

"Use Warp as an additional agent on the same session" has no home today:

- **Roster agents** (header **Add agent**) are OpenAI-compatible model loops (`base_url` + `model`),
  not interactive CLIs.
- **Peer sessions** on the same worktree were removed from the UI on 2026-08-29.
- **Terminal tabs** run only the login shell. `StartTerminalSessionRequest` has no field to say
  anything else, and the daemon hardcodes kind `"bash"`.

A terminal tab is already the right shape: a PTY in the session's worktree, kept mounted across tab
switches, closable, listed by `ListTerminalSessions` with a `kind`. What is missing is the choice of
*what* to run in it, and the CLI harness registry already answers "how do I launch harness X".

## Proposed Changes

### What's changing

**`StartTerminalSession` takes a kind**

- `StartTerminalSessionRequest.kind` (new field 4). It is either empty or `"bash"`, which keeps
  today's login-shell tab, or a registered CLI harness session type (`claude-cli`, `cursor-cli`,
  `opencode-cli`, `muse-cli`, `warp-cli`).
- An agent kind spawns that harness's **fresh interactive start**, as its descriptor builds it, in the
  session's worktree:
  - no model, no first prompt and no resume;
  - a fresh session id for a harness that takes one (Claude Code's `--session-id`);
  - no chat id for Cursor, which starts a new chat.
  
  The binary is resolved exactly as for that harness's own sessions: its env var, then
  `cli_harnesses.<type>.binary_path`, then the default name on `PATH`.
- `ListTerminalSessions` reports an agent tab with `kind` set to the harness's session type.

Refusals:

| Request | Status |
|---|---|
| `kind` is neither empty, `bash`, nor a registered harness | `INVALID_ARGUMENT` naming the kind |
| an agent kind on a **room-backed** (tddy-coder) session | `INVALID_ARGUMENT` — "this session's terminals run bash only" |
| any tab on a **sandboxed** session | `FAILED_PRECONDITION` (unchanged) |
| the harness binary cannot be spawned | the spawn error, surfaced as today for a bash tab |

**Web tab strip**

- `sessions-terminal-tab-new` (`+`) opens a menu, `sessions-terminal-tab-new-menu`. Its items are
  **Bash** (`sessions-terminal-tab-new-bash`), then one per CLI harness
  (`sessions-terminal-tab-new-<session type>`), labelled as in the Create pane's session-type toggle
  (Claude CLI, Cursor CLI, OpenCode, Muse Code, Warp). Choosing an item calls `StartTerminalSession`
  with that kind and focuses the new tab. **Escape** or a click outside closes the menu without
  starting anything.
- On a **tool** (room-backed) session the menu offers **Bash** only.
- Tabs are labelled by kind with a per-kind ordinal: **Bash 1**, **Bash 2**, **Warp 1**, **Claude
  CLI 1**. This replaces today's label (the terminal id with dashes as spaces), which shows a UUID for
  every daemon tab. Labels are rebuilt from `ListTerminalSessions` on reattach.
- Close, focus, keep-mounted, full-screen and per-terminal input routing are unchanged. An agent tab
  is a terminal tab.

### What's staying the same

- The Agent tab (`main`) and the session's own main harness.
- Bash tabs, apart from their label. An empty `kind` is byte-for-byte today's request.
- Tabs are **in-memory**. An agent tab is not persisted and does not resume after a daemon restart, a
  session resume, or a page reload that the daemon outlived (that last case reattaches as today).
- Like bash tabs, agent tabs get **no session environment**: no git identity, no `TDDY_SOCKET`, no
  provider route. They are not stopped by `DeleteSession` or by the main agent's exit. This
  pre-existing tab behaviour is recorded, not changed.
- The header **Add agent** button (roster agents) and conversation tabs.

## Impact Analysis

### Technical Impact

| Package | Change |
|---|---|
| `tddy-terminal-rpc` | Proto field `kind = 4`. `TerminalRoster::start` takes a launch request carrying the kind and the resolved login shell. The in-memory test roster records kinds |
| `tddy-session-lifecycle` | `CliManagerTerminalRoster` resolves a kind through `cli_harness(kind)` and `DaemonConfig::cli_harness_binary`, and builds the argv from the descriptor |
| `tddy-cli-sessions` | `CliSessionManager` gains an agent-terminal start that passes the kind and argv to `spawn_tool` under a fresh terminal id |
| `tddy-coder` | `CoderTerminalRoster` refuses a non-bash kind |
| `tddy-web` | Generated `terminal_session_pb.ts`. `useSessionTerminals` keeps `{ terminalId, kind }` and `open(kind)`. A menu in `SessionTerminalTabs` and kind-aware labels. The fake terminal backend records the requested kind |

Proto change, backward compatible: an old client sends no `kind` and gets bash.

### User Impact

- The `+` gains one click: choose **Bash** or an agent.
- Bash tab labels change from an id to **Bash N**.

## Implementation Plan

1. Proto field and the `TerminalRoster::start` launch request, with the three roster
   implementations. RPC-level tests in `tddy-terminal-rpc`.
2. The daemon roster: kind → descriptor → argv → spawn. Real-PTY tests in `tddy-session-lifecycle`
   with stub harness binaries.
3. The coder roster refusal, and the two-server parity test.
4. Web: hook, menu, labels, fake backend. Cypress component specs.

## Acceptance Criteria

- [ ] `StartTerminalSession` with an empty `kind` still starts the login shell, with kind `bash`.
- [ ] With `kind = "warp-cli"` (or any registered harness) it starts that harness's fresh interactive argv in the session's worktree, under a fresh terminal id.
- [ ] `ListTerminalSessions` lists that tab with the harness's session type as its `kind`.
- [ ] An unknown kind is refused `INVALID_ARGUMENT` naming it, and nothing is spawned.
- [ ] A room-backed session refuses an agent kind and still starts bash.
- [ ] The harness binary is resolved through its env var and `cli_harnesses.<type>.binary_path`, as for its own sessions.
- [ ] Stopping an agent tab kills its process and removes it from the list.
- [ ] Web: `+` opens a menu of Bash plus every CLI harness. Choosing one starts a terminal of that kind and focuses its tab.
- [ ] Web: tabs read **Bash 1** / **Warp 1** etc., and keep those labels after reattach from `ListTerminalSessions`.
- [ ] Web: a tool session's menu offers Bash only.
- [ ] Web: a refused start shows its reason in `sessions-terminal-error` and opens no tab.

## Out of scope

- Persisting or resuming agent tabs.
- A model, first prompt or provider account for an agent tab.
- Agent tabs inside a sandboxed session's jail.
- Hooks or activity status for an agent tab. A claude-cli tab inside a claude-cli session shares the
  worktree's `.claude/settings.local.json` hooks, so its activity may be reported against the main
  session. This is a known limitation, recorded as a TODO.
- Agent-to-agent messaging.
