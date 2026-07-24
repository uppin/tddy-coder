/**
 * HostStatsFooter — the screen-level bottom strip on `SessionsDrawerScreen`. Hosts the relocated
 * byte-traffic readout (`StatusBar`) plus two host-level indicators for the currently selected
 * daemon: available disk space and per-core CPU utilization.
 *
 * PRD: `docs/ft/web/host-stats-footer.md`
 * Changeset: `host-stats-footer`
 */

import React from "react";
import type { SessionAttachmentState } from "./useSessionAttachment";
import { StatusBar } from "./StatusBar";
import { DiskSpaceIndicator } from "./DiskSpaceIndicator";
import { CpuCoresIndicator } from "./CpuCoresIndicator";
import { UploadProgressIndicator } from "./UploadProgressIndicator";
import { useHostStats } from "../../rpc/useHostStats";
import type { SessionRuntimeRegistry, SessionRuntimeState } from "./sessionRuntimeRegistry";

export interface HostStatsFooterProps {
  /** The current session attachment — drives the relocated traffic readout (`StatusBar`). */
  attachment: SessionAttachmentState;
  /** All mounted session runtimes (focused + backgrounded) — passed to `StatusBar` so the traffic
   *  readout aggregates every attached session's terminal (data-plane) bytes. */
  runtimes?: ReadonlyArray<SessionRuntimeState>;
  /** The runtime registry backing `runtimes` — passed to `StatusBar` for live aggregate rates. */
  runtimeRegistry?: SessionRuntimeRegistry | null;
}

export function HostStatsFooter({ attachment, runtimes = [], runtimeRegistry = null }: HostStatsFooterProps) {
  const { perCorePercent, disk } = useHostStats();

  return (
    <div
      data-testid="host-stats-footer"
      className="flex-shrink-0 flex items-center gap-3 px-2 py-1 border-t border-border"
    >
      <StatusBar attachment={attachment} runtimes={runtimes} runtimeRegistry={runtimeRegistry} />
      <DiskSpaceIndicator availableBytes={disk ? disk.availableBytes : null} />
      <CpuCoresIndicator perCorePercent={perCorePercent} />
      <UploadProgressIndicator />
    </div>
  );
}
