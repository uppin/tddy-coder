/**
 * The decision a Hosts row makes before it subscribes: which host, if any, its telemetry cell reads.
 *
 * A pure function rather than an expression inline in the component, because it is the one property
 * a component test cannot see — every row shares one transport and the request names no host, so a
 * cell reading the selected daemon for every row renders identical DOM.
 *
 * PRD: `docs/ft/web/1-WIP/PRD-2026-09-06-telemetry-fanout.md` (AC-3, AC-4, AC-5)
 */

/** What the row knows about its host by the time it decides. */
export interface HostTelemetryRow {
  instanceId: string;
  /** In the live roster. */
  online: boolean;
  /** Some registered wire reaches it. */
  routable: boolean;
}

/**
 * The host this row's feed should read, or `null` for no feed at all.
 *
 * Never `undefined`: `useHostStats(undefined)` follows the daemon selector, which would put the
 * selected daemon's CPU under this row's host name — a fabricated reading an operator would act on.
 */
export function telemetryFeedFor(row: HostTelemetryRow): string | null {
  return row.online && row.routable ? row.instanceId : null;
}
