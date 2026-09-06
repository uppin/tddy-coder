# 2026-09-05 — A host that has no video does not offer a video tab

Everything in the dashboard that is built on LiveKit **tracks** or **participants** now renders only
where the selected host is actually reached over a wire that carries them. On a host that is not,
the VNC and Screen Sharing tabs are gone from the session inspector, the LiveKit screen and its
menu entry are gone from navigation, the rooms panel and the RPC Playground's participant picker are
gone from their screens, and the participant roster says why it is empty instead of promising to
connect for ever.

Gone, not greyed out. A tab the operator cannot use is worse than a tab that is not there: a
disabled VNC tab invites a support question with no good answer, while an absent one matches what is
actually true of the connection. Where an entry point has to stay — the `#/livekit` route, so a
shared link still lands somewhere; the participant panel, which is the only place a join failure gets
reported — it stays and names the reason instead.

Two things that used to look identical are now told apart. **A join still in flight** is not an
absent capability, so nothing appears on a LiveKit page and then withdraws itself a second later —
which is what made the inspector's tab strip reflow under the cursor on every page load. And **a
join that failed** keeps every surface that would report the reason, rather than replacing an ICE
failure with a verdict about the wire.

The sessions drawer degrades honestly rather than silently. Cross-host rows come from participant
presence, so without it the list collapses to the selected host's own sessions — which reads exactly
like "nothing is running anywhere else". The drawer now keeps the rows it has and adds a footnote
under them saying sessions on other hosts are not visible from this connection.

Node 4 of the `optional-livekit` stack, on the host directory in
[#438](https://github.com/uppin/tddy-coder/pull/438) and the session connections in
[#439](https://github.com/uppin/tddy-coder/pull/439). Folding the terminal's own room join into the
session connection follows in [#441](https://github.com/uppin/tddy-coder/pull/441), and the first
host reached without LiveKit at all in [#443](https://github.com/uppin/tddy-coder/pull/443).

Feature [daemon-selector-livekit-rpc.md](../daemon-selector-livekit-rpc.md),
[app-shell.md](../app-shell.md), [session-drawer.md](../session-drawer.md),
[livekit-rooms-panel.md](../livekit-rooms-panel.md), [vnc-sessions.md](../vnc-sessions.md),
[screen-sharing-sessions.md](../screen-sharing-sessions.md); technical
[capability-gating.md](../../../../packages/tddy-web/docs/capability-gating.md).
