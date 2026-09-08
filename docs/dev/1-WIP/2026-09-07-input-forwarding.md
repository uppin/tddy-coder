# Changeset: input-forwarding

**Date:** 2026-09-07
**Status:** 🟢 Green — implemented, all tests passing
**Type:** New feature (completes an existing one)
**Stack:** `#hosts-screen` 9/9 — the top node
**Branch:** `feature/hosts-screen/input-forwarding` → base `feature/hosts-screen/desktop-connect`

## Related feature documentation

PRD: [`docs/ft/web/1-WIP/PRD-2026-09-07-input-forwarding.md`](../../ft/web/1-WIP/PRD-2026-09-07-input-forwarding.md)

Affected: [`screen-sharing-sessions.md`](../../ft/web/screen-sharing-sessions.md) — its `AC-SS-6`
specifies this behaviour and currently records it as not reachable from the browser. This node makes
that criterion true and removes the caveat.

Background: [`docs/dev/todo/2026-09-07-remote-desktop-input-forwarding.md`](../todo/2026-09-07-remote-desktop-input-forwarding.md)

## Affected packages

| Package | Change |
|---|---|
| [`packages/tddy-web`](../../../packages/tddy-web) | input capture + the `ScreenSharingInputService` client, in `ScreenSharingOverlay` |

One package. Nothing below the browser changes.

## Responsibility

- A **browser client for `ScreenSharingInputService`**, opened against the bridge participant over
  the LiveKit data channel for the lifetime of an open overlay.
- **Pointer capture** on the overlay's video element — move, button press, button release — scaled
  from rendered element space to framebuffer space.
- **Keyboard capture**, mapped to the neutral keysym values the proto carries.
- A decision, and its implementation, for **which keys the overlay keeps** rather than forwards.
- **Both scopes at once**: the host desktop and the per-session desktop, since both mount the same
  overlay.

## Boundaries

- Does **not** change `packages/tddy-screenshare` — the bridge serves the service, drains the
  stream and injects the events already. **If this node finds itself editing the bridge, it has
  misunderstood the problem.**
- Does **not** change `screen_sharing_input.proto` or regenerate it. The message shape is fixed:
  `pointer{x, y, button_mask}`, `key{keysym, pressed}`, empty acks.
- Does **not** touch `packages/tddy-vnc`, `packages/tddy-rdp`, or the daemon's bridge spawn path.
- Does **not** change how a stream is started or stopped — `StartHostStream`, `StartStream` and
  their overlays' lifecycles are node 8's and the session path's.
- Does **not** add audio, clipboard, file transfer, multi-monitor, or touch/pen input.
- Does **not** revive `VncOverlay.tsx`, `vncInput.ts` as a module, or `vnc_input.proto`. Lifting the
  tested scaling and keysym **logic** out of `vncInput.ts` is fine and expected; reviving the
  superseded surface around it is not.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `desktop-connect` (#hosts-screen 8/8) | `HostDesktopOverlay` mounting `ScreenSharingOverlay` at host scope, with the bridge identity and track name from `StartHostStream` | forwards input on a host desktop through the same mounted overlay | change the host target model, the start/stop RPCs, the password prompt or the media gate |
| pre-existing (not a stack node) | `ScreenSharingOverlay`, the per-session tab, and `tddy-screenshare`'s served `ScreenSharingInputService` | adds capture to the overlay; opens a client against the already-serving bridge | reimplement the bridge, the injection path, or the proto |

⚠ The bridge half is **already built and shipped**. The single most likely way to get this node
wrong is to conclude it is missing and write a second one.

## Draft PR contract

Top node — nothing branches off it. Its first push is still the API surface plus failing tests, so
it is reviewable early:

1. The input-client module's seam (open, send, close) with its signature settled.
2. The failing acceptance tests below.

## Green wave

**Wave:** 1 of 1
**Greenable independently:** yes — every test mounts this node's own overlay against a fake bridge
that records the events it received. Nothing here needs another node's behaviour at runtime; node 8
supplies a mount point that already exists on its branch.
**Concurrent with:** nothing — it is the only unimplemented node left.
**Blocks:** nothing.

## Scope

- [x] `ScreenSharingInputService` client wired to the bridge participant's data channel
- [x] Pointer capture with coordinate scaling
- [x] Keyboard capture with keysym mapping
- [x] The keep-vs-forward key policy
- [x] Stream opened on mount, closed on unmount
- [x] Input-unavailable state that leaves the picture working
- [x] Cypress component tests for both scopes — written, failing (see **Red phase** below)

## Decisions — settled before the red phase

1. **The overlay keeps exactly one chord, `Ctrl+Alt+Esc`, and forwards everything else** — Escape
   included. Keeping Escape would make the desktop useless for vim, dialogs and most full-screen
   applications; forwarding everything with no keyboard exit would trap the operator. The close
   button and backdrop click are unchanged, so the chord is a convenience and not the only way out,
   and the overlay names it on screen. ⚠ Browser-reserved combinations (`Cmd+W`, `Ctrl+W`, `F5`,
   `Cmd+Tab`) cannot be captured by a page at all and will never reach the desktop; the UI must not
   imply otherwise.
2. **Plain coordinate mapping, not Pointer Lock.** It is what `AC-IF-3` describes and what
   `vncInput.ts`'s tested scaling was written for. Pointer Lock adds a permission prompt and its own
   escape-hatch problem for a relative-motion benefit no acceptance criterion asks for. Revisit only
   if a real desktop proves unusable without it.
3. **Pointer moves are coalesced to at most one per animation frame.** A browser emits pointer-move
   far faster than a desktop can consume it, and the bridge's `try_recv` drain loop discards the
   backlog anyway — so sending it is pure data-channel cost. Not pinned by an acceptance test: a
   frame-rate assertion in Cypress buys a flaky test for a property no operator can observe.

## Technical debt this node should consider

The `vnc_*` generation — `packages/tddy-vnc`'s input methods, `vnc_input.proto`, `VncOverlay.tsx`,
`vncInput.ts` — is superseded and referenced by nothing in production. It has now misled two
separate readings of this feature in opposite directions: `VncOverlay.tsx`'s docblock claims it
"Captures pointer and keyboard events" when its code does not, which first made input forwarding
look like a reuse and then made an audit conclude the feature had never been built. At minimum this
node fixes that docblock. Deleting the surface outright is worth proposing, as its own change.

## Testing plan

| Level | Where | Why |
|---|---|---|
| Component (Cypress) | `packages/tddy-web/cypress/component/` | Capture, scaling and the keep-vs-forward policy are UI contracts; a fake bridge that records received events is the natural double |
| Unit | `packages/tddy-web/src/**` | Coordinate scaling and keysym mapping are pure functions and should be pinned as such, not only through a mounted component |

⚠ **Assert the events the bridge received, never that a handler ran.** A test that asserts an
`onMouseDown` fired proves nothing about whether anything reached the wire. `#hosts-screen 8/8`
produced seven tests that passed while proving nothing; the recurring cause was asserting a
mechanism rather than an outcome. Every test here asserts what the fake bridge received.

## Red phase — the failing tests, and what each one would catch

Written 2026-09-07. **No production behaviour was added**; the only production file created is the
seam below, whose two exports throw.

| File | Tests | Covers |
|---|---|---|
| [`packages/tddy-web/src/components/sessions/screenSharingInput.test.ts`](../../../packages/tddy-web/src/components/sessions/screenSharingInput.test.ts) | 15 | AC-IF-2, AC-IF-3 as pure functions |
| [`packages/tddy-web/cypress/component/ScreenSharingInputForwardingAcceptance.cy.tsx`](../../../packages/tddy-web/cypress/component/ScreenSharingInputForwardingAcceptance.cy.tsx) | 11 | AC-IF-1, 2, 3, 6, 7, 8, 9 |
| [`packages/tddy-web/cypress/component/ScreenSharingInputScopesAcceptance.cy.tsx`](../../../packages/tddy-web/cypress/component/ScreenSharingInputScopesAcceptance.cy.tsx) | 2 | AC-IF-4 (host), AC-IF-5 (session) |

Supporting test infrastructure:

- `cypress/support/rpc/screenSharingInputBridge.ts` — the fake bridge. It **serves**
  `ScreenSharingInputService` and records every `ScreenSharingInputEvent` that arrives on the
  stream, plus how many streams were opened and how many have ended. Every assertion in both specs
  is about what it received; none is about a handler having run.
- `cypress/support/pages/screenSharingOverlayPage.ts` — the overlay's gestures, expressed as an
  offset *inside the rendered picture* so a spec never hands the component the coordinate it is
  supposed to compute.

Seam added so the tests compile: `src/components/sessions/screenSharingInput.ts`, exporting
`framebufferPointFor` and `keysymFor`, both of which throw `… is not implemented`.

⚠ Every one of these 28 tests was confirmed **reachable**: a throwaway implementation of the seam
plus input capture in `ScreenSharingOverlay` turned all 28 green and left the 48 tests of
`HostDesktopConnectAcceptance`, `HostsScreenRemoteDesktopAcceptance`, `HostAddKeyAcceptance`,
`HostsScreenAddKeyAcceptance` and `SessionScreenSharingTargetRowsAcceptance` green as well. That
implementation was then reverted; the probe existed only to rule out a test that cannot pass.

## Green phase — what was implemented

Landed in two milestones, each pushed only once its own tests were green.

**1. The two pure translations** (`791d98ef`) — `screenSharingInput.ts`. The scaling formula and the
keysym table were lifted from the superseded `vncInput.ts` as this document expects: no import from
it, none of its surface revived, and the keys the tests do not pin (`Home`, `End`, `PageUp`,
`PageDown`, `Insert`, `Meta`, `CapsLock`) kept, since dropping them would leave the surviving
surface worse than the one it replaces. Two departures from the lifted logic, both corrections:
code points above `0xff` are encoded as X11 actually encodes them, `0x01000000 | codePoint` — the
old code returned the bare code point, so `ą` came out as keysym `0x105`, which is not a keysym for
`ą`; and the table is read with `Object.hasOwn` rather than `in`, so a browser reporting
`constructor` cannot return a function through a `number` return type.

**2. The client and capture** — `useScreenSharingInput.ts` (new), wired into `ScreenSharingOverlay`.

| Decision | Why it is not the obvious alternative |
|---|---|
| Transport via `useLiveKitTransportFactory` + `useLiveKitTransportFactoryIsOverridden` | `useLiveKitClient` null-guards on `room`, and the overlay is mounted with none. Mirrors `useSessionUsage.ts` exactly — it keys off how the RPC provider was configured, not off a test environment |
| One stream per `client`, closed by `AsyncQueue.close()` on unmount | Ending the request iterable is what the bridge sees as the desktop closing. Keyed on `[client]` alone, so a re-render is not a reconnect |
| `getBoundingClientRect()` read on **every** event | A factor fixed at mount puts the operator's clicks somewhere they did not aim the moment the window is resized |
| Nothing forwarded from a `0×0` picture | `framebufferPointFor` yields `NaN` there, and `NaN` is not a number a `uint32` field can carry. Guarded at the call site, not inside the pure function |
| `rfbButtonMaskFor` reads `MouseEvent.buttons` (the held set) | `button_mask` says what is down *now*: a release is the remaining buttons, and a drag carries its button along with every move |
| Its own bit mapping rather than `vncInput.ts`'s | That helper maps `MouseEvent.button`, the index. Browser and RFB disagree in the middle, so reusing it bit-for-bit would have turned every right-click into a middle-click paste |
| A pending move is flushed *before* a button, never dropped | A desktop told the button went down before it was told the pointer arrived acts on the wrong place |
| Keyboard listener on `document` | A `<video>` takes no focus, so keys typed over the overlay arrive nowhere else |
| No `preventDefault()` for a key with no keysym | Nothing is being sent, so there is nothing to swallow; the browser keeps the key rather than having it vanish into a desktop it never reached |

### Resolved from the red phase's notes

- `ScreenSharingOverlay.tsx`'s docblock now says it forwards input and that `Ctrl+Alt+Esc`, not
  Escape, dismisses it. `VncOverlay.tsx`'s false capture claim — the one that misled two readings of
  this feature in opposite directions — is corrected to say it forwards none.
- The overlay's `width`/`height` props, previously ignored entirely, are now the framebuffer size.
- `HostDesktopOverlay.tsx`'s `TODO(#hosts-screen 8/8)` asking for exactly this is deleted.

### Left open, deliberately

- **The overlay captures the keyboard app-wide while it is open**, including once the bridge has
  refused the stream, where a key is prevented and then goes nowhere. Right for a full-screen
  overlay and no worse than the Escape handler it replaces, but no test pins it; stopping capture
  on refusal is a defensible change if an operator ever notices.
- The testkit question this document raised — whether `InMemoryRpcBackend` should record streaming
  messages the way it records unary ones, retiring `screenSharingInputBridge.ts`'s hand-rolled
  recording — was **not** taken on. It is testkit work, not this node's, and the double is honest as
  it stands.

### Verification

| Suite | Result |
|---|---|
| `screenSharingInput.test.ts` | 15 / 15 |
| `ScreenSharingInputForwardingAcceptance.cy.tsx` | 11 / 11 |
| `ScreenSharingInputScopesAcceptance.cy.tsx` | 2 / 2 |
| `tddy-web` unit suite | 1169 / 1169 |
| Protected specs — `HostDesktopConnect`, `HostsScreenRemoteDesktop`, `SessionScreenSharingTargetRows`, `HostAddKey`, `HostsScreenAddKey`, `VncOverlay`, `SessionVncTargetRows` | 60 / 60 |

## Refactoring needed

### From @red (TDD Red Phase)

- `cypress/support/rpc/screenSharingInputBridge.ts` is a second recording double alongside
  `recordingLiveKitRpc.tsx`; the two record different things (stream payloads versus target
  identities) and the scopes spec uses both. Worth one look during green for whether the testkit
  should record streaming messages the way it already records unary ones — `InMemoryRpcBackend`'s
  interceptor explicitly skips them ("Streaming messages are not captured in v1").
- `ScreenSharingOverlay.tsx`'s own docblock already claims it "Captures pointer and keyboard events"
  and that Escape dismisses it. Green makes the first true and the second false; both lines have to
  change with the code, as does `VncOverlay.tsx`'s (see the technical-debt note above).
- The overlay currently ignores its `width`/`height` props entirely. Green is the first consumer.

## TODO

- [x] Create PRD documentation
- [x] Create changeset (this document)
- [ ] Record initial discovery
- [x] Settle the open questions above
- [x] TDD Red — failing acceptance tests
- [x] TDD Green — implement
- [ ] Validate, refactor, wrap

## Successor PRs

None — this is the top node of `#hosts-screen`.
