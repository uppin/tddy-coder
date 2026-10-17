# 2026-10-17 — A failed `ConnectSession` strands a live session on "Select Resume to reconnect"

**Type:** Bug (reported, not yet fixed — this changeset records the finding and the intended fix)

## Symptom

A session is created and the agent starts (the daemon registers the PTY, `.session.yaml` says
`status: active`, the process keeps running), but the web pane never shows its terminal. It shows
only the placeholder **"Select Resume to reconnect"**. The session is alive, so the top bar offers
no Resume (Resume is keyed on dormancy), and there is no way to claim or re-enter the terminal.
Reselecting another session and coming back is the only recovery found; the reporter ended up
signalling and resuming the process (`SignalSession` 15 → `ResumeSession`).

Observed 2026-10-17 on session `01a149ca-c090-7791-b2fe-0505e181f942` (`claude-cli`, `livekit_room: null`
in `.session.yaml`).

## Where the placeholder comes from

[`SessionMainPane.tsx`](../../../packages/tddy-web/src/components/sessions/SessionMainPane.tsx)
renders it when the pane has **no attached runtime and the attachment is not `connected`**. It
carries no information about whether the daemon considers the session alive. So the question is
why a live session ends up with no attachment and nothing that will attach it.

## Finding 1 (UI): a failed attach is never released, so nothing retries

The attach rules ([`attachClaim.ts`](../../../packages/tddy-web/src/components/sessions/attachClaim.ts))
rest on an *attach claim*: a live session with a claim is `hold` — never attached a second time —
which is what stops a `ConnectSession` per list poll. The claim is meant to be given back when the
attach did not happen.

The give-back is wired to the wrong place:

- [`useSessionAttachment.attach()`](../../../packages/tddy-web/src/components/sessions/useSessionAttachment.ts)
  **catches every error itself** and turns it into `{ status: "error" }`. The promise it returns
  therefore always resolves.
- [`SessionsDrawerScreen`](../../../packages/tddy-web/src/components/sessions/SessionsDrawerScreen.tsx)
  calls `connectSession(...).catch(...)` in the activation effect and the liveness effect, and
  `resumeSession(...).catch(...)` in `handleResume`. `handleResume`'s `.catch` is written to release
  the claim ("the resume never landed … release the claim"); in all three, the `.catch` is **dead
  code**, because the promise never rejects.
- After a failed `ConnectSession` the claim therefore stays held. `attachActionForSnapshot`
  returns `hold` for a live session that has a claim, on every later poll. The only thing the
  `error` status does is open the inspector. Nothing re-attaches, and there is no Resume button for
  a live session.

This is the same family as the 2026-08-13 incident fixed in #382
([`SessionTerminalFeedRecoveryAcceptance.cy.tsx`](../../../packages/tddy-web/cypress/component/SessionTerminalFeedRecoveryAcceptance.cy.tsx)):
there the claim outlived an evicted runtime after the *feed* dropped; here it outlives an attach
that *failed*. #382 covers `claimAfterFeedEnd` only.

## Finding 2 (suspected cause of the failure, not yet traced): an unusable LiveKit URL

The attach does fail in the reporter's logs. Each `ConnectSession` in `~/.tddy/logs/daemon` is
followed within milliseconds by a LiveKit client error in `~/.tddy/logs/webrtc`:

| `ConnectSession` | `webrtc` log |
|---|---|
| 15:28:02.040 | 15:28:02.045 `failed to connect: Signal(UrlParse("relative URL without a base"))`, 3 retries |
| 15:42:24.039 | 15:42:24.099 same error, 3 retries |

The daemon's configured URL is a well-formed `wss://…` (`~/.tddy/desktop.yaml`, `livekit.url`), so
the relative/empty URL is coming from somewhere between the `ConnectSession` reply and
`attachmentHintFromReply` / `host.openSession`. The session's `.session.yaml` has
`livekit_room: null` even though the daemon logs it is hosting `session-<id>`, which is the first
thing to look at. **This has not been traced** — the correlation is by timestamp only.

Finding 1 turns any such failure from a transient error into a permanently stranded pane; Finding 2
is what produced the failure here.

## Intended fix

1. **Red:** a Cypress component test in the `SessionTerminalFeedRecoveryAcceptance` style — a live
   session whose `ConnectSession` rejects must not leave the pane stranded: the claim is released
   (or a retry/Resume affordance is offered) and a later successful attach reaches the terminal.
2. **Green:** make the failure observable to the caller — `attach()` returns/propagates its outcome
   (or the claim is released on the `error` status) so the three call sites release the claim like
   `handleResume` does. Do not paper over it with a polling retry loop: a persistently failing
   attach must stay visible (`error` + inspector), not spin.
3. Trace Finding 2: how the daemon builds the `ConnectSession` LiveKit fields for a session with
   `livekit_room: null`, and why the client opens a connection from them rather than the gRPC path.

## Out of scope

- The `subject_id` warning at session start (`github account uppin has no subject_id`): unrelated,
  logged on every spawn.
- The unrelated git-timeout warnings from `session_room` snapshots under load.
