/**
 * The label the selected daemon is known by, whether or not it is listed right now.
 *
 * PRD: docs/ft/web/url-state-routing.md § Host in the URL.
 */

import { useEffect, useMemo } from "react";
import type { DaemonHost } from "../lib/participantRole";
import {
  findListedDaemon,
  readRememberedDaemonLabel,
  rememberDaemonLabels,
} from "../routing/selectedHost";

/**
 * Remember every label `daemons` lists, and name `selectedInstanceId` by it.
 *
 * Returns the selected daemon's listed label while it is listed, otherwise the one this tab last
 * saw it listed under (`routing/selectedHost`'s label memory). `null` when nothing is selected, or
 * the selected daemon has never been listed in this tab — a shared link opened before the common
 * room lists its host.
 */
export function useSelectedDaemonLabel(
  daemons: DaemonHost[],
  selectedInstanceId: string | null,
): string | null {
  // Remember every label the directory lists, so a selected daemon that later is not listed — after
  // a reload, before the common room has joined, or once it leaves the room — still has a name.
  useEffect(() => {
    rememberDaemonLabels(daemons);
  }, [daemons]);

  // A listed daemon is named by its listing, which is newer than anything remembered: the effect
  // above records it only after this render. An unlisted one is named by the memory, which holds
  // every listing up to the previous render — including the one it was last listed in.
  return useMemo(() => {
    if (!selectedInstanceId) return null;
    const listed = findListedDaemon(daemons, selectedInstanceId);
    return listed ? listed.label : readRememberedDaemonLabel(selectedInstanceId);
  }, [daemons, selectedInstanceId]);
}
