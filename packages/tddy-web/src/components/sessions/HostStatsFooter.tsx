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

export interface HostStatsFooterProps {
  /** The current session attachment — drives the relocated traffic readout (`StatusBar`). */
  attachment: SessionAttachmentState;
}

export function HostStatsFooter({ attachment }: HostStatsFooterProps) {
  const { perCorePercent, disk } = useHostStats();

  return (
    <div
      data-testid="host-stats-footer"
      className="flex-shrink-0 flex items-center gap-3 px-2 py-1 border-t border-border"
    >
      <StatusBar attachment={attachment} />
      <DiskSpaceIndicator availableBytes={disk ? disk.availableBytes : null} />
      <CpuCoresIndicator perCorePercent={perCorePercent} />
      <UploadProgressIndicator />
    </div>
  );
}
