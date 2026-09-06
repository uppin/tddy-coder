/**
 * Live resource telemetry for one host row: per-core CPU and free disk, streaming.
 *
 * One `StreamHostStats` subscription per **online** host, opened only while this cell is mounted.
 * An offline host has no connection to subscribe through, so nothing is opened and the cell says so.
 *
 * The four states are deliberately distinct, and never collapse into a number:
 *
 * Listed in the order the branches below test them:
 *
 * | Row state                          | Cell |
 * |------------------------------------|------|
 * | offline                             | `—`, and no subscription attempted |
 * | online, but nothing routes to it    | an unavailable marker |
 * | online, subscribed, no frame yet    | a pending marker — not empty bars, which would read as idle |
 * | online, subscribed, reading in hand | live CPU bars + free disk |
 *
 * The last two are decided per metric, not for the cell as a whole: a reading that carries disk but
 * no CPU leaves the CPU slot pending rather than drawing an empty strip.
 *
 * A zeroed CPU bar for a host that is not reporting would be a fabricated reading an operator acts
 * on, so it is never rendered — see CLAUDE.md on fallbacks.
 *
 * **Which hosts are reachable is the directory's answer, not the row's.** The Hosts screen lists
 * every host tddy has a *record* of, so a row can name a machine no wire reaches; the host directory
 * (`useDaemons`) lists the hosts something can currently address. A row outside it is unreachable,
 * and asking for its stats anyway would open a stream against a host nothing routes to. That is the
 * same rule `useHostFanOut` applies when it reads peers: only advertised hosts are read, and a host
 * with no connection gets a said-so, never an empty answer dressed up as data.
 */

import { useHostConnection } from "../../rpc/connections/registry";
import { useDaemons } from "../../rpc/selectedDaemon";
import { useHostStats } from "../../rpc/useHostStats";
import { CpuCoresIndicator } from "../sessions/CpuCoresIndicator";
import { DiskSpaceIndicator } from "../sessions/DiskSpaceIndicator";
import { clampCorePercent } from "../sessions/hostStatsFormat";

export interface HostRowTelemetryProps {
  /** The host to read. */
  instanceId: string;
  /** Whether the registry reports this host in the live roster; gates whether we subscribe at all. */
  online: boolean;
}

/**
 * Per-core percentages as `data-core-{n}` attributes on the CPU cell.
 *
 * The bars themselves are `CpuCoresIndicator`'s, whose test ids are per-core and would collide
 * across rows; a reading belongs to a host, so the row's own cell is where it is addressable.
 */
function coreAttributes(perCorePercent: number[]): Record<string, string> {
  return Object.fromEntries(
    perCorePercent.map((percent, index) => [
      `data-core-${index}`,
      String(clampCorePercent(percent)),
    ]),
  );
}

export function HostRowTelemetry({ instanceId, online }: HostRowTelemetryProps) {
  const daemons = useDaemons();
  const inDirectory = daemons.some((daemon) => daemon.instanceId === instanceId);
  // Hooks cannot be conditional, so "do not subscribe" is said by naming no host: an offline row, or
  // one the directory does not name, resolves no connection and `useHostStats(null)` opens nothing.
  const connection = useHostConnection(online && inDirectory ? instanceId : null);
  const { perCorePercent, disk } = useHostStats(connection ? instanceId : null);

  // Until the first event lands there is no reading — not a reading of zero. The daemon emits its
  // snapshot on subscribe, so this is the brief window between opening the stream and its first
  // frame, and an errored or silent host stays here rather than showing idle bars.
  const hasReading = perCorePercent.length > 0 || disk !== null;

  const content = !online ? (
    <span title="Offline — no live reading">—</span>
  ) : !connection ? (
    <span
      data-testid={`hosts-row-${instanceId}-telemetry-unavailable`}
      title="No connection reaches this host"
    >
      unavailable
    </span>
  ) : !hasReading ? (
    <span data-testid={`hosts-row-${instanceId}-telemetry-pending`} title="Waiting for a reading">
      …
    </span>
  ) : (
    <>
      {/* An empty bar strip is indistinguishable from every core at 0%, so a reading that carries
          disk but no CPU shows the CPU slot as still pending rather than drawing nothing. */}
      {perCorePercent.length > 0 ? (
        <span data-testid={`hosts-row-${instanceId}-cpu`} {...coreAttributes(perCorePercent)}>
          <CpuCoresIndicator perCorePercent={perCorePercent} />
        </span>
      ) : (
        <span data-testid={`hosts-row-${instanceId}-cpu-pending`} title="Waiting for a CPU reading">
          …
        </span>
      )}
      <span data-testid={`hosts-row-${instanceId}-disk`}>
        <DiskSpaceIndicator availableBytes={disk ? disk.availableBytes : null} />
      </span>
    </>
  );

  return (
    <span
      data-testid={`hosts-row-${instanceId}-telemetry`}
      className="flex items-center gap-2 text-xs text-muted-foreground"
    >
      {content}
    </span>
  );
}
