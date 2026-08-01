/**
 * Which daemon is selected, and where that choice is remembered.
 *
 * Pure (no React, no LiveKit) so it can be unit-tested directly — it lived in the
 * `SelectedDaemonProvider` `.tsx` module before, which made it unreachable from `bun test`.
 * `rpc/selectedDaemon.tsx` re-exports everything here for its existing importers.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-01-url-state-routing.md § Host in the URL.
 */

import type { DaemonHost } from "../lib/participantRole";

/** `sessionStorage` key holding this tab's selected daemon. Exported so tests seed the real key. */
export const SELECTED_DAEMON_STORAGE_KEY = "tddy_selected_daemon";

/** The selected daemon's instance id, persisted for this browser tab, or `null` if never set. */
export function readStoredSelectedDaemon(): string | null {
  return sessionStorage.getItem(SELECTED_DAEMON_STORAGE_KEY);
}

/** Persist the selected daemon's instance id for this browser tab. */
export function writeStoredSelectedDaemon(instanceId: string): void {
  sessionStorage.setItem(SELECTED_DAEMON_STORAGE_KEY, instanceId);
}

/**
 * Resolve which daemon should be selected, in precedence order:
 * 1. `urlInstanceId` (the `?host=` param), if it is still among `daemons`.
 * 2. `storedInstanceId`, if it is still among `daemons`.
 * 3. `servingInstanceId` (the daemon that served this web bundle), if still among `daemons`.
 * 4. The first daemon in `daemons`, if any.
 * 5. `null`, when there are no daemons in the room yet.
 *
 * The URL leads because it is the only source a shared link carries: `sessionStorage` is per-tab,
 * so a colleague opening the link — or the same operator opening it in a new tab — would otherwise
 * land on a different host, where every session id in that link means nothing.
 */
export function resolveSelectedDaemonInstanceId(params: {
  daemons: DaemonHost[];
  servingInstanceId?: string;
  storedInstanceId?: string | null;
  urlInstanceId?: string | null;
}): string | null {
  const { daemons, servingInstanceId, storedInstanceId, urlInstanceId } = params;
  const presentIds = new Set(daemons.map((d) => d.instanceId));
  if (urlInstanceId && presentIds.has(urlInstanceId)) return urlInstanceId;
  if (storedInstanceId && presentIds.has(storedInstanceId)) return storedInstanceId;
  if (servingInstanceId && presentIds.has(servingInstanceId)) return servingInstanceId;
  if (daemons.length > 0) return daemons[0].instanceId;
  return null;
}
