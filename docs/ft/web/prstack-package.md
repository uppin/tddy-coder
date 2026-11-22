# PR-stack package (`tddy-workqueue-web`)

**Package:** `packages/tddy-workqueue-web`
**Applies to:** the PR-Stack screen and its chat, and the workflow chat screen in the session drawer
**Added:** 2026-11-07 (#web-split 14/17, PR #850)

## Overview

The PR-stack orchestrator's screens are one package: the planned-PR list and panel with live branch,
PR and base-sync status, reorder and repoint, starting a planned PR's session, the orchestrator chat and
the workflow chat screen. It reaches daemons only through `tddy-client-web`, renders with `tddy-ui-web`,
opens sessions through `tddy-session-create-web` and chat through `tddy-chat-web`; it does not depend on
the web app or on LiveKit.

## Behaviour

- PR-stack behaviour is as before ([pr-stacking.md](../coder/pr-stacking.md),
  [pr-stack-live-status.md](../coder/pr-stack-live-status.md),
  [session-drawer.md § PR-Stack Chat Screen](session-drawer.md)).
- The queue view (`WorkQueueView`) shows one session's own work queue, addressed by that session's
  id, and creates no queue: a queue exists only because its session was started with one, and a
  `session_id`-addressed mutation from a session that owns none is refused with the hint
  ([pr-stacking.md § Opening a session's work queue](../coder/pr-stacking.md#opening-a-sessions-work-queue)).
- An expanded work-item row assigns an existing PR by number, removes it, links a session from the
  session list and unlinks one; a refusal is shown inline on that row. An item with an assigned PR
  shows its checks' running, failed and succeeded counts and, expanded, each check as the session PR
  tab's `CheckRow` (this package depends on `tddy-workspace-web` for it), kept live by the queue's
  `StreamWorkQueueStatus` — never polled
  ([pr-stacking.md § An item's assigned PR and linked sessions](../coder/pr-stacking.md#an-items-assigned-pr-and-linked-sessions-added-2026-11-22)).

## Acceptance criteria

- [x] PR-stack lives in `tddy-workqueue-web` and imports nothing from `tddy-web` or LiveKit (rule test).
- [x] Its specs pass from the package (`web.yml` runs them; `Web lint` typechecks it).
- [x] The drawer renders PR-stack sessions as before (`tddy-web`'s drawer-driven PR-stack specs pass).

Technical reference: [packages/tddy-workqueue-web/docs/README.md](../../../packages/tddy-workqueue-web/docs/README.md).
