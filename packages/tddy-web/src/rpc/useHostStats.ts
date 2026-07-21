/**
 * Polling hooks for host-level machine stats surfaced by the Host Stats Footer: per-core CPU
 * utilization (fast poll) and the free/total disk capacity of the selected daemon's default
 * project directory (slow poll).
 *
 * Both target the currently selected daemon's `ConnectionService` over the shared common-room
 * LiveKit connection (`useDaemonClient`), so they follow the daemon selector like every other
 * daemon-level readout.
 *
 * PRD: `docs/ft/web/host-stats-footer.md`
 * Changeset: `host-stats-footer`
 */

import { useEffect, useState } from "react";
import { ConnectionService } from "../gen/connection_pb";
import { useDaemonClient } from "./selectedDaemon";
import { useAuthContext } from "../hooks/authProvider";

/** CPU is polled fast so the footer's mini bars track load in near-real-time. */
export const CPU_REFRESH_MS = 5000;
/** Disk capacity changes slowly — a once-a-minute poll is plenty. */
export const DISK_REFRESH_MS = 60000;

/** Free/total disk capacity for the selected daemon's default project directory. */
export interface HostDiskStats {
  availableBytes: bigint;
  totalBytes: bigint;
  projectDir: string;
}

/**
 * Per-core CPU utilization percentages (core 0 first) for the selected daemon, re-fetched every
 * {@link CPU_REFRESH_MS}. Empty until the first response arrives (or while no daemon is selected).
 */
export function useHostCpuStats(): number[] {
  const client = useDaemonClient(ConnectionService);
  const { sessionToken } = useAuthContext();
  const [perCorePercent, setPerCorePercent] = useState<number[]>([]);

  useEffect(() => {
    if (!client) return;
    let cancelled = false;
    const fetchOnce = () => {
      client
        .getHostCpuStats({ sessionToken: sessionToken ?? "" })
        .then((resp) => {
          if (!cancelled) setPerCorePercent(resp.perCorePercent);
        })
        .catch((err) => {
          console.debug("[useHostCpuStats] getHostCpuStats error", err);
        });
    };
    fetchOnce();
    const interval = setInterval(fetchOnce, CPU_REFRESH_MS);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [client, sessionToken]);

  return perCorePercent;
}

/**
 * Free/total disk capacity for the selected daemon's default project directory, re-fetched every
 * {@link DISK_REFRESH_MS}. `null` until the first response arrives (or while no daemon is selected).
 */
export function useHostDiskStats(): HostDiskStats | null {
  const client = useDaemonClient(ConnectionService);
  const { sessionToken } = useAuthContext();
  const [disk, setDisk] = useState<HostDiskStats | null>(null);

  useEffect(() => {
    if (!client) return;
    let cancelled = false;
    const fetchOnce = () => {
      client
        .getHostDiskStats({ sessionToken: sessionToken ?? "" })
        .then((resp) => {
          if (cancelled) return;
          setDisk({
            availableBytes: resp.availableBytes,
            totalBytes: resp.totalBytes,
            projectDir: resp.projectDir,
          });
        })
        .catch((err) => {
          console.debug("[useHostDiskStats] getHostDiskStats error", err);
        });
    };
    fetchOnce();
    const interval = setInterval(fetchOnce, DISK_REFRESH_MS);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [client, sessionToken]);

  return disk;
}
