# 2026-10-23 — Session status streams (activity, usage, notifications) do not reconnect after a failure

**Category:** Deferred from `web-dev` LiveKit and visible errors (#706)
**Source:** `packages/tddy-web/src/components/sessions/useSessionActivity.ts`, `useSessionUsage.ts`,
`packages/tddy-web/src/rpc/useSessionNotifications.ts`

## What happens

When one of these server streams ends with an error, #706 shows that error inline: "Usage unavailable: …" in the
Usage tab, and "Session notifications unavailable: …" in the drawer strip. Nothing re-subscribes, though. The error
stays up, and the data stays frozen, until something else starts a new subscription: a different client, session,
token or room.

## Why deferred

The streams never reconnected before #706 either. They failed silently, so the frozen data simply went unnoticed.
#706's job was to make the failure visible. Adding reconnection changes behaviour (backoff, duplicate snapshots,
interaction with token refresh) and needs a design of its own.

## What would close it

A shared reconnect-with-backoff helper for server streams, used by all three hooks, that clears the error on the
first message after reconnecting. Cypress coverage: the stream fails, then the error shows, then the stream recovers,
then the error clears and new data renders.
