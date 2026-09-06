# Per-host telemetry on the Hosts screen

Every row of the Hosts screen carries a live reading of that host's per-core CPU and free disk,
streamed rather than polled.

## Motivation

The daemon has always produced this data, but only the **selected** host's feed was ever consumed —
by the Host Stats Footer. An operator comparing machines had to select each one in turn and read the
footer, which is the opposite of what a fleet list is for. A row that shows its own load answers
"which machine is busy" at a glance.

## What a row shows

| Row state | Telemetry cell |
|---|---|
| Online, connected, reporting | per-core CPU bars and free disk, updating as readings arrive |
| Online, connected, not yet reported | a pending marker |
| Online, but no wire reaches it | an unavailable marker |
| Offline | an offline marker, and no subscription is attempted |

The three non-reading states are deliberately distinct and never collapse into a number. A zeroed CPU
bar for a host that is not reporting is a reading an operator would act on, so it is never drawn —
including the narrower case of a reading that carries disk but no CPU, where the disk figure stands
and the CPU slot alone stays pending.

The cell reuses the same indicators as the Host Stats Footer, so a bar means the same thing on both
surfaces.

## Subscription policy

- **One subscription per online, routable host** — not one per render, and none for a host that is
  offline or that nothing routes to.
- **Screen-scoped.** Subscriptions open while the Hosts screen is mounted and are cancelled when it
  is left. N hosts at a 5 s cadence is real load, and nothing should pay it while another screen is
  in front of the operator.
- **A host's reading belongs to that host.** A row never falls back to the selected daemon's figures.

## Relationship to the Host Stats Footer

Both read `ConnectionService.StreamHostStats` through the same hook. The footer follows the daemon
selector and is unaffected by this screen: it still opens exactly one subscription, against the
selected host. See [`host-stats-footer.md`](./host-stats-footer.md).

## Acceptance criteria

- [x] An online host's row shows its per-core CPU bars and free disk, sourced from a live stream.
- [x] The row updates as fresh readings arrive, without a reload.
- [x] Exactly one subscription is opened per online host — not one per render.
- [x] An offline host's row shows no reading and opens no subscription.
- [x] A host that is connecting, errored, or unreachable shows a pending or unavailable state, never
      a fabricated zero.
- [x] Leaving the Hosts screen cancels every subscription it opened.
- [x] The Host Stats Footer is unchanged — still one subscription against the selected host.

## Technical reference

[`packages/tddy-web/docs/host-stats-streaming.md`](../../../packages/tddy-web/docs/host-stats-streaming.md)
