/**
 * Live resource telemetry for one host row: per-core CPU and free disk, streaming.
 *
 * One `StreamHostStats` subscription per **online** host, opened only while this cell is mounted.
 * An offline host has no connection to subscribe through, so nothing is opened and the cell says so.
 *
 * The three states are deliberately distinct, and never collapse into a number:
 *
 * | Row state                     | Cell |
 * |-------------------------------|------|
 * | online, connected             | live CPU bars + free disk |
 * | online, connecting or errored | a pending / unavailable marker |
 * | offline                       | `—`, and no subscription attempted |
 *
 * A zeroed CPU bar for a host that is not reporting would be a fabricated reading an operator acts
 * on, so it is never rendered — see CLAUDE.md on fallbacks.
 */

export interface HostRowTelemetryProps {
  /** The host to read. */
  instanceId: string;
  /** Whether the registry reports this host in the live roster; gates whether we subscribe at all. */
  online: boolean;
}

export function HostRowTelemetry({ instanceId, online }: HostRowTelemetryProps) {
  // TODO(telemetry-fanout): implement — subscribe via useHostStats(instanceId) when `online`,
  // render CpuCoresIndicator + DiskSpaceIndicator, and the honest non-values otherwise.
  void instanceId;
  void online;
  return <span data-testid={`hosts-row-${instanceId}-telemetry`} />;
}
