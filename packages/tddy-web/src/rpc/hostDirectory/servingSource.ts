/**
 * The daemon serving this page, as a host-directory source.
 *
 * `/api/config` has always named it (`daemon_instance_id`), and `SelectedDaemonProvider` has always
 * received it — but only ever as a *tie-breaker* for which of the common room's daemons to select.
 * With no common room the list was empty and the selector offered nothing, so a page whose own
 * daemon is one process away showed an operator no host at all. Contributing it as a source is what
 * makes "no LiveKit" a working configuration rather than an empty screen.
 *
 * It is also the entry the desktop's IPC source (node 6 of the `optional-livekit` stack) later
 * replaces with a richer descriptor: same host id, more that is known about it.
 */

import { useMemo } from "react";
import { SELF_LABEL_SUFFIX } from "../../lib/participantRole";
import { hostDescriptorOf } from "./daemonHost";
import type { HostDirectorySource } from "./types";

/** The id this source contributes under. Precedence is stated against it, so it is a constant. */
export const SERVING_SOURCE_ID = "serving";

/**
 * The serving daemon's contribution to the directory.
 *
 * `connected` whenever there is a serving instance id: the page was served by that daemon, so its
 * existence is not in question — this source names a host, it does not claim a wire to it. Whether
 * anything can *reach* it is the connection registry's answer, and each screen already renders its
 * own "no connection to this host" state when nothing can.
 *
 * `idle` with no hosts when there is no id, which is what a bundle served by something that is not
 * a daemon (a static file server, a Storybook build) reports.
 */
export function useServingHostDirectorySource(
  servingInstanceId: string | undefined,
): HostDirectorySource {
  return useMemo<HostDirectorySource>(() => {
    const hostId = servingInstanceId?.trim() ?? "";
    if (!hostId) return { id: SERVING_SOURCE_ID, status: "idle", error: null, hosts: [] };
    return {
      id: SERVING_SOURCE_ID,
      status: "connected",
      error: null,
      hosts: [
        hostDescriptorOf(
          { instanceId: hostId, label: `${hostId}${SELF_LABEL_SUFFIX}` },
          SERVING_SOURCE_ID,
        ),
      ],
    };
  }, [servingInstanceId]);
}
