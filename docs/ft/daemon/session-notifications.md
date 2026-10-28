# Session notifications — one bus, several surfaces

**Status:** Shipped
**Updated:** 2026-10-26

## Overview

When something happens in a coding session that an operator should know about — the agent needs an
answer, it finished a turn, it is working — the daemon publishes a **session notification**.
Subscribers declare which kinds they want. The daemon's **operator channel** — the chat an operator
is reached on, which is **Telegram** when one is configured — is one; **`tddy-web`'s drawer
indicators** are another. The operator channel is also where a workflow session's elicitations are
delivered, with their keyboards, so both of a session's ways of reaching its operator end at one
place.

Telegram used to *be* the notification system rather than a surface on top of one, and it named
sessions by their uuid prefix while the web named them by worktree — so a chat message could not be
matched to a row in the drawer.

## One label, everywhere

An **activity alert** names a session the way the web session drawer names it: the basename of its
`repo_path`, falling back to its `workflow_goal`, falling back to the first eight characters of its
id. A session working in `/home/dev/my-feature-branch` reads as `Session my-feature-branch` in a
chat *and* on its drawer row.

The other Telegram surfaces — the metadata-tick status line, presenter elicitation, the `/sessions`
list, chain-parent buttons — still use the short-id label. Unifying those is a tracked follow-up.

## What is notified

| Kind | Raised by | Telegram | Drawer dot |
|---|---|---|---|
| `ATTENTION_REQUIRED` | `WaitingForInput`, `Done` | ✓ (unchanged copy) | yellow |
| `ATTENTION_REQUIRED` | a claude-cli permission prompt or `AskUserQuestion` (source `PENDING_QUESTION`, carrying `question`) | ✓ *(names the tool, or lists the options)* | yellow |
| `ATTENTION_REQUIRED` | a presenter elicitation | ✓ *(via its own keyboard surface)* | yellow |
| `ATTENTION_REQUIRED` | the agent itself, with `tddy-tools notify` (source `AGENT`) | ✓ *(rendered markdown, see below)* | yellow |
| `ACTIVITY` | `Started` / `Running` / `ExecutingTool`, an agent tool call, a workflow state change | — | blinking green |

**One notification per question.** A report that records a question publishes one
`PENDING_QUESTION` notification and no separate status one; its drawer text is
`🔔 Session {label} wants to run {tool}: {summary}` or `🔔 Session {label} asks: {first question}`.
The `WaitingForInput` status that follows the same prompt carries the same `question`, and Telegram
declines it, so the operator is told once.

**Telegram traffic did not grow.** `ACTIVITY` exists for indicators; sending it to a chat would turn
every tool call into a message.

## Agent-written notifications

Every other notification is a fixed sentence the daemon writes. An agent in a daemon-launched
claude-cli or cursor-cli session can instead tell its operator, in its own words, what finished or
what is blocked:

```bash
tddy-tools notify "Parallel run finished" --body "4 done, **1 blocked**" --level success --sound
```

- **Arguments.** A one-line title; an optional markdown `--body` (`--body -` reads it from stdin); a
  `--level` of `info` (default), `success`, `warning` or `error`; `--sound` to alert audibly.
- **Limits are refusals, never truncation.** The title must be 1–200 characters after trimming and
  the body at most 2,000. A message over a limit is refused with a reason naming the part that was
  too long, so the agent learns it and can say it again, shorter.
- **Not fail-quiet.** Unlike the hooks, the command prints `Notification sent` on success and exits
  non-zero with the reason when the environment is missing, the daemon cannot be reached, or the
  daemon refuses.
- **Credentials come from the agent's environment.** The launch exports `TDDY_SESSION_ID`,
  `TDDY_OS_USER`, `TDDY_HOOK_TOKEN` and exactly one of `TDDY_DAEMON_SOCKET` (an embedded daemon's
  agent-tool socket) or `TDDY_DAEMON_URL`, at start and again at resume. These are the credentials the
  session's hooks already hold, so exporting them grants nothing new — see
  [Claude Code CLI session § Notifying the operator](claude-cli-session.md#notifying-the-operator).
- **Authenticated like a hook.** `ActivityService.ReportOperatorNotification` refuses a session that
  is not claude-cli or cursor-cli (`FAILED_PRECONDITION`), then a session that recorded no hook token
  or a token that does not match it (`PERMISSION_DENIED`).
- **What is published.** One `ATTENTION_REQUIRED` notification with source `AGENT`. Its `text` is the
  title, so a surface that knows nothing of agent notifications still shows one line; beside it the
  notification carries the title, the markdown body, the level, the sound flag and the session's web
  route `#/sessions/<id>?host=<daemon instance>`.
- **Never de-duplicated.** Two identical agent notifications are two events, and both are delivered.

Who cannot notify: a session with a standing role (the project-home main has no shell, so it is not
given the environment) and a sandboxed claude-cli session (it has no hooks and no hook token).

## The drawer indicator

One dot per row, four states, evaluated in this order:

| State | Rendered | Condition |
|---|---|---|
| `disconnected` | grey, steady | the session is not active, whatever it last reported |
| `needs-input` | yellow, steady | a pending elicitation, **or** an attention notification newer than the last view |
| `working` | green, **fading in and out** | activity newer than the last view, within 30 seconds |
| `connected` | green, steady | alive, nothing outstanding |

**Viewing a session settles its dot.** Selecting a row marks its notifications seen; later activity
raises the blink again, and a later attention notification raises yellow again. Reaching a session
by deep link or Back does *not* settle it — a reload landing on a session should not clear an
indicator the operator never looked at.

**A pending elicitation is not dismissible by looking at it.** That flag is a persisted, unanswered
gate; clearing it on a glance would claim the operator had dealt with something they had not. It
stays yellow until the elicitation is actually answered.

**The blink stops on its own.** Activity older than 30 seconds settles to steady green with no
further signal, so a session whose agent died mid-turn stops claiming to be working.

**Reduced motion is respected.** Under `prefers-reduced-motion: reduce` the animation is disabled and
the dot stays fully opaque — a dot frozen mid-fade would read as a different state, not a still one.

## Delivery

One daemon-level **`ActivityService.StreamSessionNotifications`** subscription serves the whole
drawer, however many rows it shows — the request names no session. It is **live-only**: a replayed
backlog would raise indicators for turns that finished while the tab was closed.

**The feed is scoped to its subscriber.** The bus behind it is host-wide, so on a daemon serving
several operators the stream is scoped to the caller's OS user; a token that maps to no OS user is
refused, and a notification whose owner cannot be established is delivered to nobody.

**An embedding host adds its own subscriber.** A process that hosts the daemon in code registers
one with `RuntimeOptions::with_notification_subscriber`; it is offered every notification after the
two built-in subscribers, in registration order. Tddy Desktop uses this for
[native notifications](../desktop/tddy-desktop-tauri.md#native-notifications). The `tddy-daemon`
binary registers none.

## Guarantees

- A notification that cannot be delivered is logged, never propagated: reporting a hook still
  succeeds when a subscriber errors, and one subscriber's failure does not starve the others.
- Bot tokens, session tokens and hook tokens never appear in a notification's text or in a log line.
- A status re-reported without changing sends no second Telegram message (an agent-written
  notification is always sent), and a presenter
  elicitation reaches Telegram once — through its keyboard surface, never also as a bus alert.
- The drawer indicator does not depend on Telegram: which presenter modes wait on the operator is
  the daemon's own rule, so a daemon with no `telegram:` block lights a workflow session's dot for
  every elicitation, and a daemon with Telegram agrees with the chat on every mode.

## Known limitation

A workflow session **started from Telegram** does not yet raise a drawer indicator from its
presenter events; sessions started or resumed from the web do. Its Telegram surface is unaffected.

## Related

- **[telegram-notifications.md](telegram-notifications.md)** — the Telegram surface in full.
- **[../web/session-drawer.md](../web/session-drawer.md)** — where the indicator is rendered.
- **[../../../packages/tddy-session-activity/docs/session-notifications.md](../../../packages/tddy-session-activity/docs/session-notifications.md)** — implementation reference.
