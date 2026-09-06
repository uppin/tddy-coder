# 2026-09-05 — LiveKit-dependent UI is gated on what the connection can actually carry

**Type:** Architecture

Node 4 of the `optional-livekit` stack ([#440](https://github.com/uppin/tddy-coder/pull/440)),
`tddy-web` only. Nodes 1–3 made daemon and session RPC transport-neutral; this node handles what RPC
cannot abstract. A frame pipe cannot carry a video track or a participant roster, so the surfaces
built on them are gated on the connection's `capabilities` rather than made wire-neutral: VNC, screen
sharing, participant video, the participant roster, the LiveKit screen and its nav entry, the rooms
panel, the RPC playground's participant picker, and the sessions drawer's cross-host reconciliation.

Three decisions are worth keeping outside `tddy-web`'s own docs, because each is the kind of thing a
later transport will be tempted to redo:

- **One predicate, and only one.** Capability information now reaches the app from three directions;
  a fourth place that answered "can I show video here" its own way would make the wire-neutral model
  decorative. `useHasCapability` is that place.
- **Two facts, in a fixed order — status, then capability.** A join in flight has produced no
  connection yet and a failed join produces none at all, so a capability-first gate reports "not
  available on this connection" during every join and then retracts it. It would also have replaced
  the reason a join failed with a verdict about the wire, which is the 2026-08-13 `udoo` incident's
  own failure mode restated.
- **Hide, don't disable.** A gated surface leaves navigation. Where an entry point must stay for
  layout or for deep-link survival, it names why it is unavailable rather than sitting there greyed
  out.

**One limitation is carried forward on purpose.** The status half of the rule is fleet-wide — one
LiveKit host directory source per page — while the capability half belongs to one host's connection.
It cannot be scoped per host today: during a join there is no `HostConnection` to read either fact
off, and a host descriptor's source id names which source advertised the host first, not which wire
would carry the capability. That is exact while one provider is registered and every connection
answers identically. The **first mixed fleet** — node 7
([#443](https://github.com/uppin/tddy-coder/pull/443)), where one host is reached over IPC and
another over a room — is where a common room stuck in `error` would keep media tabs on a host that
can never serve a track. The fix belongs in `useCapabilityAvailability`, and node 6 is the first node
in a position to define what it needs: a way to ask which source would carry a given capability for
a given host.

No proto change, no daemon change, no new npm dependency.

Technical
[capability-gating.md](../../../packages/tddy-web/docs/capability-gating.md), package changeset
[2026-09-05-capability-gating.md](../../../packages/tddy-web/docs/changesets/2026-09-05-capability-gating.md),
product [daemon-selector-livekit-rpc.md](../../ft/web/daemon-selector-livekit-rpc.md).
