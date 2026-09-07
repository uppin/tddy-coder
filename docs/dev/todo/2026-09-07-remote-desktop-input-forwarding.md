# 2026-09-07 — remote desktop input forwarding

**Category:** Missing feature (both ends built, whole middle absent)
**Source:** `#hosts-screen 8/8` desktop-connect changeset, 2026-09-07 — recorded here because that
node's AC-3 was deferred and its 1-WIP documents are wrapped away when the PR is readied.

- **Input forwarding has never worked, for host-scoped *or* per-session desktops.** A connected
  desktop is **view-only** everywhere in tddy today. This is easy to miss, because
  `packages/tddy-web/src/components/sessions/VncOverlay.tsx`'s docblock claims it "Captures pointer
  and keyboard events" — **it does not.** Its only key and mouse handlers are Escape-to-close and
  click-outside-to-close, exactly like `ScreenSharingOverlay.tsx` (lines 65-85). That stale docblock
  is what led `#hosts-screen 8/8` to plan input forwarding as a *reuse*, which cost a round of
  investigation to disprove. **Fix the docblock even if nothing else here is done.**

- **What actually exists, layer by layer** — both ends are built and the entire middle is missing:

  | Layer | State |
  |---|---|
  | Browser translate — `src/components/sessions/vncInput.ts` (coordinate scaling, X11 keysym map) | exists, unit-tested, **no production consumer** — only its own `vncInput.test.ts` |
  | Browser capture and send | missing |
  | Wire — `packages/tddy-service/proto/vnc_input.proto`, `VncInputService.StreamInput` (bidi) | proto defined, **implemented by neither end** |
  | Daemon — serve `VncInputService`, route to the right bridge | missing |
  | Daemon → bridge channel | **none exists in any form** — a bridge reads one JSON `BridgeConfig` from stdin at startup and never reads again |
  | Bridge → server | `packages/tddy-vnc/src/vnc_client.rs` has `pointer_event` / `KeyEvent`; `packages/tddy-rdp/src/rdp_client.rs` has `MousePdu` / `FastPathInputEvent` — **nothing calls either** |

  Verified on `master`, on `feature/hosts-screen/desktop-probe`, and on all seven other
  `#hosts-screen` branches.

- **The open design question is the daemon → bridge channel**, and it should be settled in planning
  rather than during green: a second pipe on the bridge process, a unix socket, or a LiveKit data
  channel the bridge subscribes to. That choice drives the whole feature — the other layers are
  comparatively mechanical once it is made.

- **Scope note.** This spans `tddy-web`, `tddy-service`, `tddy-daemon` and `tddy-vnc`/`tddy-rdp`.
  `#hosts-screen 8/8` explicitly forbade itself the last three in its `## Boundaries`, which is why
  it deferred rather than widened. Whoever picks this up gets the per-session path fixed for free,
  since the missing middle is shared.

- **Related, smaller:** host desktop targets are auto-created on first connect and there is no UI to
  delete one. `#hosts-screen 8/8` removed its unused `RemoveHostTarget` RPC rather than ship surface
  nothing called; if targets ever become user-managed, that RPC and a row affordance come back
  together.
