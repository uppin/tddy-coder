# Changeset: input-forwarding

**Date:** 2026-09-07
**Status:** 🚧 Planning
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

- [ ] `ScreenSharingInputService` client wired to the bridge participant's data channel
- [ ] Pointer capture with coordinate scaling
- [ ] Keyboard capture with keysym mapping
- [ ] The keep-vs-forward key policy
- [ ] Stream opened on mount, closed on unmount
- [ ] Input-unavailable state that leaves the picture working
- [ ] Cypress component tests for both scopes

## Open questions — to settle before green, not during

1. **Which keys does the overlay keep?** Escape currently closes it, and a remote desktop needs
   Escape. Options: forward everything and close only via the button; keep a single chord
   (e.g. `Ctrl+Alt+Esc`) for close and forward the rest; keep Escape and offer a "send Escape"
   affordance. Whichever is chosen, the browser's own reserved chords (`Cmd+W`, `Ctrl+W`, `F5`,
   `Cmd+Tab`) cannot be captured at all, so the answer must not pretend otherwise.
2. **Pointer capture semantics.** Whether to use Pointer Lock for relative movement, or plain
   coordinate mapping. Lock feels right for a desktop but is a permission prompt and an escape-hatch
   problem of its own; plain mapping is simpler and matches what `vncInput.ts` was written for.
3. **Event rate.** A pointer-move stream at browser event rate will flood a data channel. Whether to
   throttle, coalesce to animation frames, or send on change only — and whether the bridge's
   `try_recv` drain loop already makes this a non-issue in practice.

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

## TODO

- [x] Create PRD documentation
- [x] Create changeset (this document)
- [ ] Record initial discovery
- [ ] Settle the open questions above
- [ ] TDD Red — failing acceptance tests
- [ ] TDD Green — implement
- [ ] Validate, refactor, wrap

## Successor PRs

None — this is the top node of `#hosts-screen`.
