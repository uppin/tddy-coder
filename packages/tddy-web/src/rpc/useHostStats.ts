/**
 * Streaming hook for host-level machine stats surfaced by the Host Stats Footer: per-core CPU
 * utilization and the free/total disk capacity of the selected daemon's default project directory.
 *
 * Both are sourced from a single `ConnectionService.StreamHostStats` server-stream. The daemon owns
 * the cadence (immediate emit on subscribe, then CPU every 5 s and disk every 60 s); each event
 * carries the latest CPU and disk snapshot.
 *
 * Three call shapes, one contract:
 *
 * - `useHostStats()` follows the **daemon selector**, as the Host Stats Footer has always done.
 * - `useHostStats(hostId)` subscribes to **that host**, which is what lets the Hosts screen show a
 *   live reading per row rather than only for whichever host happens to be selected.
 * - `useHostStats(null)` subscribes to **nothing**. It is how a caller says "this row gets no feed"
 *   — an offline host, or one nothing routes to — without breaking the rules of hooks.
 *
 * A `null` client never subscribes. Note the converse does not hold: a registered common room
 * answers for **any** host id (`connections/liveKit.tsx`'s `connectHost`), because roster membership
 * is the host directory's business and not a provider's. So a non-null client is not evidence the
 * host is reachable — deciding *which* hosts are worth subscribing to belongs to the caller, which
 * reads the directory. A host it does not subscribe for renders as "no reading", never as zero.
 *
 * PRD: `docs/ft/web/host-stats-footer.md`, `docs/ft/web/hosts-screen-telemetry.md`
 * Changeset: `2026-07-22-streamed-host-stats`
 */

import { useEffect, useState } from "react";
import { ConnectionService } from "../gen/connection_pb";
import { subscribeHostStats } from "./hostStatsSubscription";
import { useHostClient } from "./connections/registry";
import { useDaemonClient } from "./selectedDaemon";
import { useAuthContext } from "../hooks/authProvider";

/** Free/total disk capacity for the selected daemon's default project directory. */
export interface HostDiskStats {
  availableBytes: bigint;
  totalBytes: bigint;
  projectDir: string;
}

/** Total and available physical memory for a host. */
export interface HostMemoryStats {
  availableBytes: bigint;
  totalBytes: bigint;
}

/** 1/5/15-minute load averages, on hosts that report them. */
export interface HostLoadStats {
  oneMinute: number;
  fiveMinutes: number;
  fifteenMinutes: number;
}

export interface UseHostStatsResult {
  /** Per-core CPU utilization percentages (core 0 first). Empty until the first event arrives. */
  perCorePercent: number[];
  /** Logical core count as the host reports it, or `null` before the first event. */
  logicalCores: number | null;
  /** Latest disk figures, or `null` until the first event arrives (or while no daemon is selected). */
  disk: HostDiskStats | null;
  /** Latest memory figures, or `null` before the first event. */
  memory: HostMemoryStats | null;
  /**
   * Latest load averages, or `null`.
   *
   * `null` covers two different things on purpose — no event yet, and a host whose platform has no
   * load average — and in both cases the only honest rendering is "no reading". What it must never
   * become is `0`, which reads as an idle machine.
   */
  load: HostLoadStats | null;
}

/**
 * Subscribe once to `ConnectionService.StreamHostStats` — for `hostId` when given, otherwise for the
 * selected daemon — and expose the latest CPU and disk snapshots. The reading itself is
 * `subscribeHostStats`: unmounting closes the stream even when the host has never reported, which a
 * `for await` parked on its first frame could not do.
 *
 * @param hostId a specific host to read; `null` to subscribe to nothing; **omitted** to follow the
 *        daemon selector. `null` and `undefined` are deliberately not the same: collapsing them
 *        (`hostId ?? undefined`) would report the selected daemon's CPU under an unreachable host's
 *        name, which is the fabrication the tri-state exists to prevent.
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

    // A reading belongs to the client that produced it. Without this reset a host that went away
    // and came back would re-show its pre-outage CPU and disk as a live reading until the new feed's
    // first frame — and indefinitely if the reconnected host never reports.
    setPerCorePercent([]);
    setDisk(null);

    const subscription = subscribeHostStats(
      (signal) => client.streamHostStats({ sessionToken: sessionToken ?? "" }, { signal }),
      (event) => {
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
      },
    );

    return () => subscription.unsubscribe();
  }, [client, sessionToken]);

  return { perCorePercent, disk };
}
