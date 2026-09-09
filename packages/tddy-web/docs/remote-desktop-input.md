# Remote desktop input

How the browser turns a watched desktop into a usable one: `useScreenSharingInput`, the client for
`ScreenSharingInputService`, and the two pure translations it makes before anything reaches the wire.

**Product requirements:** [`docs/ft/web/screen-sharing-sessions.md`](../../../docs/ft/web/screen-sharing-sessions.md)
§ AC-SS-6.

## Where it lives

| File | Holds |
|---|---|
| `src/components/sessions/screenSharingInput.ts` | `framebufferPointFor`, `keysymFor`, `rfbButtonMaskFor` — pure, total, unit-tested |
| `src/components/sessions/useScreenSharingInput.ts` | the stream's lifetime, event capture, coalescing, the closing chord |
| `src/components/sessions/ScreenSharingOverlay.tsx` | mounts the hook, renders the picture, the chord label and the unavailable notice |

One overlay serves both scopes — a host's desktop and a session's — so both get the same input
behaviour from the same code. There is no host-scoped or session-scoped input path.

## The bridge half already existed

`packages/tddy-screenshare/src/bridge.rs` serves `ScreenSharingInputService` over its LiveKit data
channel and injects every event into the protocol client. Nothing in this package needs to change
that, and **a reader who concludes the feature is unbuilt because no browser code sends anything is
reading a stale tree** — that was true until `#hosts-screen 9/9`.

Three properties of the bridge that the client's design depends on, each verifiable in `bridge.rs`:

- It **drains and injects the whole backlog** before sampling each frame (`while let Ok(cmd) =
  input_rx.try_recv()`). It discards nothing, so an event sent is an event paid for.
- It **acks every event**. The reply stream is not idle; the client ignores the acks.
- `stream_input` **always answers `Ok`**. A bridge that cannot inject drops commands silently. So a
  view-only server is never *reported* as one.

## The two translations

**Coordinates.** `framebufferPointFor` scales a pointer from the rendered element's pixels into
framebuffer pixels, rounding to the nearest whole one. The element's box is read on **every event**,
never cached: a factor fixed at mount is wrong the moment the window is resized, and the operator's
clicks then land where they did not aim.

The far edge maps one past the last addressable pixel — `{960,540}` in a 960×540 element showing a
1920×1080 desktop is `{1920,1080}`, while the framebuffer addresses 0–1919. This is deliberate and
not clamped: clamping would move every edge aim inward by a pixel, and the remote server clamps its
own input anyway. The far-right column is where scrollbars and close buttons live, so aiming at it
has to work.

**Keysyms.** `keysymFor` maps a browser `KeyboardEvent.key` to the neutral keysym the proto carries:
a table for keys whose `key` is a name, otherwise the character's own code point — `A` and `a` are
two different keysyms, not one plus a modifier flag. Two details that are easy to get wrong and are
pinned by tests:

- Code points above Latin-1 are `0x01000000 | codePoint`, X11's encoding. Returning the bare code
  point is wrong for everything outside Latin-1.
- The table is read with `Object.hasOwn`, not `in`. `key` is whatever string the browser reported,
  and `"constructor"` would otherwise find a function on the prototype chain and return it through a
  `number`.

A key with no keysym is **dropped** — nothing is sent, and `preventDefault` is not called either, so
the browser keeps a key that was never going anywhere.

**Buttons.** `rfbButtonMaskFor` reads `MouseEvent.buttons` — the set currently held, not the button
that triggered the event — because `button_mask` says what is down *now*: a release is the buttons
remaining, and a drag carries its button along with every move.

⚠ It is **not** a bit-for-bit copy of the mask helper in the superseded `vncInput.ts`. That one maps
`MouseEvent.button`, the index, and the two orderings disagree in the middle: browsers number
primary/secondary/auxiliary, RFB orders left/middle/right. Copying it across turns every right-click
into a middle-click paste.

## The stream

Opened when the overlay mounts, closed when it unmounts, one per bridge participant. A re-render is
not a reconnect — the client is memoized on the factory, room and bridge identity.

**The stream is opened eagerly, with one empty event, and that line is load-bearing.** The LiveKit
transport publishes the `call` frame — the one that makes the peer dispatch the method — only with
the *first enqueued request message* (`packages/tddy-rpc-web/src/envelope-transport.ts`). Iterating
the reply side publishes nothing. Without the opener the bridge would not learn of the stream until
the operator's first mouse move, and the "input unavailable" notice could not appear until after
they had already clicked into a surface that ignored them. An event with no `event` case set is a
safe opener: the bridge skips it (`None => continue`).

`useSessionUsage` primes its stream for the same reason. **If you copy this shape again, copy the
opener with it** — this hook shipped once without it, and every acceptance test still passed,
because the in-memory Cypress testkit runs the handler as soon as the response iterator is pulled.
No component test in this package can tell a primed stream from an unprimed one.

A stream that fails, or that ends on its own, is **closed** and raises the notice. Without the close
the outbound queue keeps accepting events for a dead call — it is unbounded and the transport's
publish is fire-and-forget, so every later pointer move is still encoded and put on the wire.

## What is released, and when

**Everything still held is let go of on teardown, and on window blur.** The overlay's advertised way
out is `Ctrl+Alt+Esc`, which forwards `Control↓` and `Alt↓` and *then* closes — so without this the
chord itself leaves both modifiers down on the remote machine, and every later keystroke there
arrives as a Ctrl+Alt chord. The same applies to a button held when the overlay closes, and to
tabbing away mid-keypress.

⚠ **The release is flushed in the stream effect's cleanup, immediately before the queue closes — not
in the capture effect's.** React runs effect cleanups in the order the effects were defined, and the
stream effect is defined first: a release enqueued from the capture cleanup would land in a queue
that is already closed, and be dropped. The held-key set therefore lives in a ref above both
effects. An acceptance test pins this, and it fails if the release is moved.

`mouseup` listens on the **window**, not the picture. The picture keeps its aspect ratio inside a
full-screen container, so there is letterbox around it: press inside, drag out, release there, and a
picture-bound listener never sees the release — the desktop drags for ever. A release outside is
reported at the last point the pointer was seen *inside* the picture, because scaling a point
outside the element gives a framebuffer coordinate off the end of the desktop.

## Capture details

- **Keyboard listens on `document`.** A `<video>` takes no focus, so keys typed while the overlay is
  up arrive nowhere else. The overlay therefore swallows every mapped key app-wide while it is open.
- **Pointer moves are coalesced to one per animation frame.** A browser reports motion far faster
  than the data channel carries it or the desktop acts on it. A pending move is **flushed before**
  any button event, never dropped: a desktop told the button went down before it was told the
  pointer arrived acts on the wrong place.
- **`contextmenu` is prevented** on the picture. The right-button press is forwarded and the remote
  desktop opens its own menu; the browser's would land on top of it.
- **Nothing is forwarded from a zero-sized picture** — the scaling would be `NaN`, which a `uint32`
  field cannot carry. The guard is at the call site, not inside the pure function.

## Not forwarded

Scroll wheel, audio, clipboard, file transfer, multi-monitor, touch and pen. Wheel is the closest to
free: RFB expresses it as buttons 4 and 5, which `button_mask` already carries.

## Superseded neighbours

`src/components/sessions/vncInput.ts` holds the same scaling and keysym logic for the retired
`VncOverlay` generation. It is imported only by its own test. The logic here was lifted from it
deliberately rather than shared, because that generation is being retired and a shared module would
tie the surviving surface to it. Deleting `vncInput.ts`, its test and `VncOverlay.tsx` is worth
doing as its own change.
