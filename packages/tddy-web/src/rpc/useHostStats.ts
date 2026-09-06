/**
 * Streaming hook for host-level machine stats surfaced by the Host Stats Footer: per-core CPU
 * utilization and the free/total disk capacity of the selected daemon's default project directory.
 *
 * Both are sourced from a single `ConnectionService.StreamHostStats` server-stream. The daemon owns
 * the cadence (immediate emit on subscribe, then CPU every 5 s and disk every 60 s); each event
 * carries the latest CPU and disk snapshot.
 *
 * Two call shapes, one contract:
 *
 * - `useHostStats()` follows the **daemon selector**, as the Host Stats Footer has always done.
 * - `useHostStats(hostId)` subscribes to **that host**, which is what lets the Hosts screen show a
 *   live reading per row rather than only for whichever host happens to be selected.
 *
 * A host with no reachable connection resolves to a `null` client and therefore **never subscribes**
 * — the caller renders that as "no reading", never as zero.
 *
 * PRD: `docs/ft/web/1-WIP/PRD-2026-07-22-streamed-host-stats.md`
 * Changeset: `2026-07-22-streamed-host-stats`
 */

import { useEffect, useState } from "react";
import { ConnectionService } from "../gen/connection_pb";
import { useHostClient } from "./connections/registry";
import { useDaemonClient } from "./selectedDaemon";
import { useAuthContext } from "../hooks/authProvider";

/** Free/total disk capacity for the selected daemon's default project directory. */
export interface HostDiskStats {
  availableBytes: bigint;
  totalBytes: bigint;
  projectDir: string;
}

export interface UseHostStatsResult {
  /** Per-core CPU utilization percentages (core 0 first). Empty until the first event arrives. */
  perCorePercent: number[];
  /** Latest disk figures, or `null` until the first event arrives (or while no daemon is selected). */
  disk: HostDiskStats | null;
}

/**
 * Subscribe once to `ConnectionService.StreamHostStats` — for `hostId` when given, otherwise for the
 * selected daemon — and expose the latest CPU and disk snapshots. The subscription/cleanup shape
 * mirrors `useSessionActivity`: a `cancelled` flag plus a cleanup that stops iterating, swallowing
 * the unmount AbortError.
 *
 * @param hostId a specific host to read, or omitted/`null` to follow the daemon selector.
 */
export function useHostStats(hostId?: string | null): UseHostStatsResult {
  const selectedClient = useDaemonClient(ConnectionService);
  const hostClient = useHostClient(ConnectionService, hostId ?? null);
  // `undefined` means "follow the selector"; an explicit id — even one nothing can reach — means
  // "that host and no other", so a null host client must not silently fall back to the selected
  // daemon and report another machine's CPU under this row.
  const client = hostId === undefined ? selectedClient : hostClient;
  const { sessionToken } = useAuthContext();
  const [perCorePercent, setPerCorePercent] = useState<number[]>([]);
  const [disk, setDisk] = useState<HostDiskStats | null>(null);

  useEffect(() => {
    if (!client) return;
    let cancelled = false;

    (async () => {
      try {
        for await (const event of client.streamHostStats({ sessionToken: sessionToken ?? "" })) {
          if (cancelled) break;
          setPerCorePercent(event.cpu?.perCorePercent ?? []);
          if (event.disk) {
            setDisk({
              availableBytes: event.disk.availableBytes,
              totalBytes: event.disk.totalBytes,
              projectDir: event.disk.projectDir,
            });
          } else {
            setDisk(null);
          }
        }
      } catch (err) {
        // A stream aborted on unmount surfaces as an AbortError; ignore it. Any other error while
        // still mounted leaves the last-known readouts in place (no fallback fabrication).
        if (!cancelled) {
          console.debug("[useHostStats] streamHostStats error", err);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [client, sessionToken]);

  return { perCorePercent, disk };
}
