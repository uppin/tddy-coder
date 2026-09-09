# 2026-09-09 — remote desktop input forwarding

The browser client for `ScreenSharingInputService`. The bridge has served the service and injected
events since the feature was first built; what never existed was anything in the browser that opened
the stream. `#hosts-screen 9/9`.

**Added**

- `src/components/sessions/screenSharingInput.ts` — `framebufferPointFor`, `keysymFor` and
  `rfbButtonMaskFor`: pure, total, and unit-tested apart from the overlay.
- `src/components/sessions/useScreenSharingInput.ts` — the stream's lifetime, pointer and keyboard
  capture, per-frame move coalescing, the closing chord, and the release of everything held.
- `docs/remote-desktop-input.md` — how it is built and which parts are load-bearing.

**Changed**

- `ScreenSharingOverlay` forwards input on both scopes, names the `Ctrl+Alt+Esc` chord on screen,
  and shows an input-unavailable notice beside a picture that keeps working. Its `width`/`height`
  props, previously ignored, are the framebuffer size. **Escape is now forwarded instead of
  dismissing the overlay** — a deliberate behaviour change to the pre-existing session path.

**Worth knowing before touching it**

- **The stream is opened with one empty event, deliberately.** The LiveKit transport publishes the
  `call` frame only with the first enqueued request message, so an unprimed stream never reaches the
  bridge. `useSessionUsage` primes for the same reason. No component test in this package can catch
  its absence: the in-memory testkit opens eagerly either way. This hook shipped once without the
  opener and every test passed.
- **Held keys are released in the stream effect's cleanup, not the capture effect's.** React runs
  cleanups in definition order, so anything the capture cleanup enqueues lands in a closed queue.
- `rfbButtonMaskFor` is not a copy of `vncInput.ts`'s mask helper. That one maps `MouseEvent.button`
  and the browser and RFB orderings disagree in the middle, so copying it swaps middle and right.
- `vncInput.ts` is now dead — imported only by its own test. Deleting it, `VncOverlay.tsx` and
  `vnc_input.proto` is worth doing as its own change.
