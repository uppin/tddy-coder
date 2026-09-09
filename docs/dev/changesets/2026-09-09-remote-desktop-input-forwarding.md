# 2026-09-09 — remote desktop input forwarding

The top node of `#hosts-screen` (9/9). One package changes — `tddy-web` — and nothing below the
browser does: the daemon, the bridge and `screen_sharing_input.proto` were already complete.

**What landed:** a browser client for `ScreenSharingInputService`, opened at the bridge participant
for the lifetime of an open overlay, with pointer and keyboard capture on both the host-scoped and
session-scoped desktops. `AC-SS-6` in
[`docs/ft/web/screen-sharing-sessions.md`](../../ft/web/screen-sharing-sessions.md) is now true, and
the four places that recorded a connected desktop as view-only are corrected. The backlog entry
[`docs/dev/todo/2026-09-07-remote-desktop-input-forwarding.md`](../todo/2026-09-07-remote-desktop-input-forwarding.md)
is marked resolved in place, keeping the links that node 8's wrapped entries already make to it;
scroll-wheel forwarding is the one part of it left open.

**The design and its traps:**
[`packages/tddy-web/docs/remote-desktop-input.md`](../../../packages/tddy-web/docs/remote-desktop-input.md).

## Two lessons this node paid for

**A shape copied from a working hook can drop the one line that made it work.** The stream client
mirrors `useSessionUsage`, and reproduced everything except its priming enqueue — the line that
forces the transport to publish the `call` frame. The result opened no stream on a real wire until
the operator's first mouse move, and **every acceptance test passed**, because the in-memory testkit
runs the handler as soon as the response iterator is pulled. Two acceptance criteria were green
against the double's transport and false against the production one. When copying a transport
idiom, copy its compensations, and ask which of its lines the test double makes unnecessary.

**A remote-input client's defects live where no browser test can reach.** The audit before this PR
went up found seven, all in the gap between a dispatched DOM event and a real desktop: the exit
chord left Ctrl and Alt held on the far machine; a button released in the letterbox around the
picture never came up; a failed stream published for ever into a dead call. Cypress dispatches
events straight at the element and records what arrived, so all of it passed. What caught them was
reading the implementation against `bridge.rs` and the transport — not more tests. The tests came
after, once it was known what to pin.

## Third lesson, for the documents

Three docblocks in the new code asserted bridge behaviour that `bridge.rs` contradicts — that it
discards its input backlog, that it answers nothing, that it refuses a stream it cannot serve. This
stack has now produced that failure three times, `VncOverlay.tsx`'s "Captures pointer and keyboard
events" being the first, and it has cost a wrong reading of the feature in both directions. A
comment that justifies a design decision by describing another component is a claim about that
component, and it should be checked against it.
