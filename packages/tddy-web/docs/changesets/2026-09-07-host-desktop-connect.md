# 2026-09-07 — A connect action on the Hosts row, and the overlay it opens

**Type:** Feature

Top node of the `#hosts-screen` stack ([#460](https://github.com/uppin/tddy-coder/pull/460)). See
the cross-package entry for the whole change:
[docs/dev/changesets/2026-09-07-host-desktop-connect.md](../../../../docs/dev/changesets/2026-09-07-host-desktop-connect.md).

`HostRowRemoteDesktop` gains `connectableDesktop(readings)` and a `hosts-row-<id>-connect-desktop`
button. All three of the predicate's conditions come from the probe — `ProbeOutcome.OK`,
`desktopReachable`, `canBridge` — and none is redundant: a reading that could not be made is no
finding, a desktop nothing serves has nothing to stream, and a reachable desktop on a daemon with no
bridge binary is a spawn failure the operator would meet after asking. The fourth condition is not a
probe fact: `useHasCapability(useHostConnection(instanceId), "media")`, the one predicate, never
re-derived from a `Room` or a transport. Without media the button is **absent, not disabled** —
`IPC_CAPABILITIES` is `{"rpc"}`, so a track cannot arrive on such a host however reachable its
desktop is.

`HostDesktopOverlay` is a thin host-scoped mount of `ScreenSharingOverlay`. No browser-side VNC/RDP
protocol client is added, ever — the picture is a daemon-produced LiveKit track. It opens on the
click rather than on the reply, because a remote spawn takes a visible interval and a failure needs
somewhere to be reported; it is portalled to `document.body`, because the row section it is opened
from is a `<span>` and a `<div>` inside one is hoisted on hydration and disturbs the row's inline
flow while open. `targetForProbedEndpoint` matches an existing host target on `(host, port,
protocol)` and adds one only when none matches, so a target does not accumulate per connect. Start
on mount, stop on unmount against the target the start actually used.

`sessionToken` is passed explicitly on all four host-scoped calls. `rpc/authGatedTransport.ts`
rewrites the field only where a request already carries one, so an omitted token is rejected on
arrival rather than repaired in flight — without it every LiveKit-reached host answered
`unauthenticated`. It is read from a ref at call time so re-minting the access token mid-desktop
does not tear the bridge down and start a second one.

The password question is subscribed to only while this overlay's own start is in flight, and
**latched** rather than derived from the live feed: the feed is per host, not per surface, so a key
passphrase prompt arriving mid-answer would otherwise unmount the dialog, lose the typing and leave
the host's call blocked. Only `HostPromptKind.DESKTOP_PASSWORD` is latched — answering another
surface's question with a desktop password would send a secret where nobody asked for one. The
dialog is the shared host passphrase dialog with additive `wording` and `allowEmpty` props, both
defaulting to the existing behaviour; empty is allowed because the host asks on every start and
plenty of desktops have no password, and refusing it would lock those out entirely. The plaintext
never reaches `HostDesktopOverlay` — the dialog returns ciphertext, after checking the published key
against the one pinned for this host.

⚠ A connected desktop is **view-only**: `src/gen/screen_sharing_input_pb.ts` is imported nowhere and
`ScreenSharingOverlay`'s only handlers are Escape-to-close and click-outside-to-close, on this scope
and the per-session one alike. Tracked at
[`docs/dev/todo/2026-09-07-remote-desktop-input-forwarding.md`](../../../../docs/dev/todo/2026-09-07-remote-desktop-input-forwarding.md).

`cypress/component/HostDesktopConnectAcceptance.cy.tsx` covers the action, both gates, the overlay
and the password prompt. Every gating spec is a contrast — positive half asserts the action exists,
negative half asserts it is withdrawn — mounted over a stated wire (`aHostConnection(HOST)` with
`room={null}`), because an absence assertion alone passes against a component that renders the
action in no condition at all.

Components [hosts-screen.md](../hosts-screen.md); the gate
[capability-gating.md](../capability-gating.md).
