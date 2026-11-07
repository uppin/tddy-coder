/**
 * The footer `SessionsDrawerScreen` shows: the host-level indicators of `tddy-hosts-web`'s
 * `HostStatsFooter`, with the session traffic readout before them and the upload progress after.
 *
 * PRD: `docs/ft/web/host-stats-footer.md`
 */

import React from "react";
import { HostStatsFooter } from "tddy-hosts-web";
import type { SessionAttachmentState } from "./useSessionAttachment";
import { StatusBar } from "./StatusBar";
import { UploadProgressIndicator } from "./UploadProgressIndicator";
import type { SessionRuntimeRegistry, SessionRuntimeState } from "./sessionRuntimeRegistry";

export interface SessionHostStatsFooterProps {
  /** The current session attachment — drives the relocated traffic readout (`StatusBar`). */
  attachment: SessionAttachmentState;
  /** All mounted session runtimes (focused + backgrounded) — passed to `StatusBar` so the traffic
   *  readout aggregates every attached session's terminal (data-plane) bytes. */
  runtimes?: ReadonlyArray<SessionRuntimeState>;
  /** The runtime registry backing `runtimes` — passed to `StatusBar` for live aggregate rates. */
  runtimeRegistry?: SessionRuntimeRegistry | null;
}

export function SessionHostStatsFooter({ attachment, runtimes = [], runtimeRegistry = null }: SessionHostStatsFooterProps) {
  return (
    <HostStatsFooter
      leading={<StatusBar attachment={attachment} runtimes={runtimes} runtimeRegistry={runtimeRegistry} />}
      trailing={<UploadProgressIndicator />}
    />
  );
}
