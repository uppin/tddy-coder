# Agent-written notifications - PRD

**Date**: 2026-10-25
**PRD Type**: Enhancement
**Stack**: `#home-inbox 1/8` (branch `feature/home-inbox/agent-notify`)

## Affected Features

- **Primary**: [Session notifications](../session-notifications.md) - a third kind of notification, written by the agent: a title, a markdown body, a level, a sound, and a link to the session
- **Related**: [Telegram session notifications](../telegram-notifications.md) - shows an agent-written notification with its markdown rendered (HTML parse mode), split into chunks within Telegram's limit
- **Related**: [Claude Code CLI session](../claude-cli-session.md) - the agent's terminal gets the credentials `tddy-tools notify` needs, plus one paragraph of guidance telling it when to notify

## Summary

An agent running in a daemon-launched claude-cli or cursor-cli session can notify its operator with:

```bash
tddy-tools notify "Parallel run finished" --body "4 done, **1 blocked**" --level success --sound
```

The daemon puts it on the session-notification bus. Telegram shows it with its markdown rendered.
Later nodes add native desktop notifications (`#home-inbox 4/8`) and the project-home agent's inbox
(`#home-inbox 7/8`).

## Background

Every notification the daemon sends today is a fixed sentence written by the daemon: "needs your
input" or "finished this turn". An agent can't say *what* finished, what is blocked, or what it
needs. Superset (analysed 2026-10-25) solves this with `superset notifications show "<title>" --body …`:

- the agent runs the command itself;
- the click target comes from the terminal's environment;
- the text is the agent's own, shown verbatim.

This PRD adopts the same model.

## Proposed Changes

### What's Changing

- **`tddy-tools notify <title> [--body <markdown>|-] [--level info|success|warning|error] [--sound]`.**
  - The body can be read from stdin.
  - The command exits non-zero with a reason the agent can read when the daemon refuses or can't be
    reached. Unlike the hooks, it is not fail-quiet.
- **Limits.**
  - The title is trimmed and must be 1–200 characters.
  - The body is at most 2,000 characters.
  - Anything over a limit is refused, never truncated.
- **Credentials come from the environment.** A daemon-launched claude-cli or cursor-cli session's
  terminal gets `TDDY_SESSION_ID`, `TDDY_OS_USER`, `TDDY_HOOK_TOKEN`, and either `TDDY_DAEMON_URL` or
  `TDDY_DAEMON_SOCKET`.
  - These are the same credentials its hooks already use, so this adds no new authority.
  - Resume exports them again.
- **A new RPC, `ActivityService.ReportOperatorNotification`,** authenticated exactly like
  `ReportSessionStatus`.
  - It publishes an `ATTENTION_REQUIRED` notification with the new source `AGENT`.
  - The notification carries a title, body, level, sound flag, and the session's web route
    (`#/sessions/<id>?host=<daemon instance>`).
- **Telegram delivers agent-written notifications.**
  - Layout: the title in bold, the session label, the body rendered from markdown to Telegram's HTML
    subset, and the session link.
  - Bodies over 4,096 bytes are split.
  - `--sound` off sends silently (`disable_notification`).
  - Agent-written notifications are never de-duplicated: two identical messages are two events.
- **Agent guidance.** A daemon-launched claude-cli or cursor-cli session gets one appended
  system-prompt paragraph:
  - `tddy-tools notify` exists;
  - use it when long work finishes or something blocks while the operator is likely away;
  - never use it for routine progress.

### What's Staying the Same

- The fixed status notifications, their wording and their de-duplication.
- The drawer indicator. The web ignores the new fields, and the new source raises the same yellow
  dot as other attention notifications.
- `tddy-tools session-hook` keeps its fail-quiet contract.

## Impact Analysis

- **Technical.**
  - New fields on `SessionNotification` and `SessionNotificationEvent`; one RPC; one CLI subcommand.
  - A markdown renderer for Telegram, built on the new `pulldown-cmark` dependency (approved
    2026-10-25).
  - Spawn and resume export the environment variables.
- **User.** Operators get messages that say what happened, in the agent's words.
- **Security.**
  - The hook token is already readable inside the session's worktree. Exporting it to the agent's
    process grants nothing the agent couldn't already do with the hooks.
  - The token never appears in a notification or a log line.

## Acceptance Criteria

- [ ] `tddy-tools notify` from a claude-cli session publishes an `AGENT` attention notification
  carrying the title, body, level, sound flag and session link.
- [ ] A title that is empty after trimming or longer than 200 characters, or a body longer than
  2,000 characters, is refused with a reason and publishes nothing.
- [ ] A wrong hook token is refused as permission denied. An unreachable daemon exits non-zero with
  a reason.
- [ ] Telegram receives the title in bold and the body as HTML: bold, italic, inline code, code
  blocks and links render, and other markup is escaped. A body over 4,096 bytes arrives in several
  messages. With `--sound` off the message is sent silently.
- [ ] Two identical agent notifications both reach Telegram.
- [ ] A claude-cli session's terminal, on start and after resume, carries the notify environment and
  the guidance paragraph.

## Out of scope

- An MCP `NotifyOperator` tool. The project-home main has no shell, so it can't use the CLI. That is
  recorded as a follow-up.
- Sandboxed claude-cli sessions. They have no hooks and no hook token today.
- Notification actions or buttons.
