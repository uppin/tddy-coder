# 2026-09-07 — remote desktop input forwarding: the browser never sends any

**Category:** Missing feature (daemon and bridge complete; browser client absent)
**Status:** Resolved — `#hosts-screen 9/9`, 2026-09-09. The browser client landed: see
[`../../../packages/tddy-web/docs/remote-desktop-input.md`](../../../packages/tddy-web/docs/remote-desktop-input.md)
and [`../changesets/2026-09-09-remote-desktop-input-forwarding.md`](../changesets/2026-09-09-remote-desktop-input-forwarding.md).
Scroll-wheel forwarding is the one part of this entry still open.
**Source:** `#hosts-screen 8/8` desktop-connect, 2026-09-07 — recorded here because that node
deferred AC-3 and its 1-WIP documents are wrapped away when the PR is readied.

- **A remote desktop is view-only, for host-scoped *and* per-session streams.** Not because the
  feature is unbuilt — the daemon and the bridge implement it fully — but because **nothing in the
  browser ever opens the input stream.**

- **What exists, and works:** `packages/tddy-screenshare/src/bridge.rs` serves
  `ScreenSharingInputService` (`packages/tddy-service/proto/screen_sharing_input.proto`) over the
  bridge's LiveKit **data channel**. `stream_input` turns each `ScreenSharingInputEvent` into an
  `InputCmd`, and the pump loop calls `client.inject_pointer(x, y, button_mask)` /
  `client.inject_key(keysym, pressed)` — the `ScreenShareClient` trait in
  `packages/tddy-screenshare/src/client.rs`, implemented for both protocols.

- **What is missing — one thing:** a browser client.
  `packages/tddy-web/src/gen/screen_sharing_input_pb.ts` is generated and **imported nowhere**, and
  `ScreenSharingOverlay.tsx`'s only pointer/keyboard handlers are Escape-to-close and
  click-outside-to-close. The work is: capture pointer and key events on the overlay's video
  element, scale coordinates from the rendered element to the framebuffer, map keys to X11 keysyms,
  and drive the bidi `StreamInput` against the bridge participant over the data channel.

- **This fixes the per-session path at the same time**, since both overlays are missing the same
  single piece. `docs/ft/web/screen-sharing-sessions.md` § AC-SS-6 already specifies the behaviour
  and describes it as though it works — the specification is right, the client was never written.

- ⚠ **Do not follow the `vnc_*` trail.** `packages/tddy-vnc`'s input methods,
  `packages/tddy-service/proto/vnc_input.proto` (`VncInputService`),
  `packages/tddy-web/src/components/sessions/VncOverlay.tsx` and its `vncInput.ts` helpers are the
  **superseded generation**, kept but unused — nothing in production references them. `vncInput.ts`
  does contain sound, unit-tested coordinate-scaling and keysym-mapping helpers that the new client
  can lift or re-derive. `VncOverlay.tsx`'s docblock claims it "Captures pointer and keyboard
  events" and its code does not; **that stale docblock is what made `#hosts-screen 8/8` plan AC-3 as
  a reuse, and then made a first audit of it conclude the feature had never been built at all.**
  Fix or delete it while you are here.

- **Also worth deciding:** whether the legacy `vnc_*` surface should be deleted outright rather than
  left to mislead a third reader.
