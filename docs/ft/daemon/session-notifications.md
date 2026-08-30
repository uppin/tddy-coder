# Session notifications — one bus, several surfaces

**Status:** Shipped
**Updated:** 2026-08-30

## Overview

When something happens in a coding session that an operator should know about — the agent needs an
answer, it finished a turn, it is working — the daemon publishes a **session notification**.
Subscribers declare which kinds they want. **Telegram** is one; **`tddy-web`'s drawer indicators**
are another.

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
| `ATTENTION_REQUIRED` | a presenter elicitation | ✓ *(via its own keyboard surface)* | yellow |
| `ACTIVITY` | `Started` / `Running` / `ExecutingTool`, an agent tool call, a workflow state change | — | blinking green |

**Telegram traffic did not grow.** `ACTIVITY` exists for indicators; sending it to a chat would turn
every tool call into a message.

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

One daemon-level **`ConnectionService.StreamSessionNotifications`** subscription serves the whole
drawer, however many rows it shows — the request names no session. It is **live-only**: a replayed
backlog would raise indicators for turns that finished while the tab was closed.

**The feed is scoped to its subscriber.** The bus behind it is host-wide, so on a daemon serving
several operators the stream is scoped to the caller's OS user; a token that maps to no OS user is
refused, and a notification whose owner cannot be established is delivered to nobody.

## Guarantees

- A notification that cannot be delivered is logged, never propagated: reporting a hook still
  succeeds when a subscriber errors, and one subscriber's failure does not starve the others.
- Bot tokens and session tokens never appear in a notification's text or in a log line.
- A status re-reported without changing sends no second Telegram message.

## Known limitation

A workflow session **started from Telegram** does not yet raise a drawer indicator from its
presenter events; sessions started or resumed from the web do. Its Telegram surface is unaffected.
Tracked in `docs/dev/TODO.md`.

## Related

- **[telegram-notifications.md](telegram-notifications.md)** — the Telegram surface in full.
- **[../web/session-drawer.md](../web/session-drawer.md)** — where the indicator is rendered.
- **[../../../packages/tddy-daemon/docs/session-notifications.md](../../../packages/tddy-daemon/docs/session-notifications.md)** — implementation reference.
