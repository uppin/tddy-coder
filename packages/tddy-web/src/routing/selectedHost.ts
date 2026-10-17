/**
 * Which daemon is selected, where that choice is remembered, and the labels this tab has seen
 * daemons listed under — so a selected daemon that is not listed right now still has a name.
 *
 * Pure (no React, no LiveKit) so it can be unit-tested directly — it lived in the
 * `SelectedDaemonProvider` `.tsx` module before, which made it unreachable from `bun test`.
 *
 * PRD: docs/ft/web/url-state-routing.md § Host in the URL.
 */

import { create, fromJsonString, toJsonString } from "@bufbuild/protobuf";
import { RememberedDaemonLabelsSchema } from "../gen/web_state_pb";
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
 * 1. `urlInstanceId` (the `?host=` param), whether or not it is among `daemons`.
 * 2. `storedInstanceId`, whether or not it is among `daemons`.
 * 3. `servingInstanceId` (the daemon that served this web bundle), if it is among `daemons`.
 * 4. The first daemon in `daemons`, if any.
 * 5. `null`, when nothing was chosen and no daemon is listed yet.
 *
 * The URL leads because it is the only source a shared link carries: `sessionStorage` is per-tab,
 * so a colleague opening the link — or the same operator opening it in a new tab — would otherwise
 * land on a different host, where every session id in that link means nothing.
 *
 * A choice (1 or 2) is kept even when its daemon is not listed. "Not listed" is not "gone": after a
 * reload the serving daemon is listed from the first render while the common room is still joining,
 * and a daemon that drops out of the room mid-session usually comes back. Replacing the choice in
 * either case moved the operator to another host — and rewrote `?host=` — without being asked. The
 * selector shows such a daemon as waiting instead; only the operator's own navigation moves away
 * from it. Only when nothing was chosen does the directory pick a default (3, 4).
 */
export function resolveSelectedDaemonInstanceId(params: {
  daemons: DaemonHost[];
  servingInstanceId?: string;
  storedInstanceId?: string | null;
  urlInstanceId?: string | null;
}): string | null {
  const { daemons, servingInstanceId, storedInstanceId, urlInstanceId } = params;
  if (urlInstanceId) return urlInstanceId;
  if (storedInstanceId) return storedInstanceId;
  if (servingInstanceId && findListedDaemon(daemons, servingInstanceId)) {
    return servingInstanceId;
  }
  if (daemons.length > 0) return daemons[0].instanceId;
  return null;
}

/** The entry for `instanceId` in `daemons`, or `undefined` when that daemon is not listed. */
export function findListedDaemon(
  daemons: readonly DaemonHost[],
  instanceId: string,
): DaemonHost | undefined {
  return daemons.find((d) => d.instanceId === instanceId);
}

// ---------------------------------------------------------------------------
// Label memory
// ---------------------------------------------------------------------------

/**
 * `sessionStorage` key holding the labels this tab has seen daemons listed under, as the protobuf
 * JSON of a `web_state.RememberedDaemonLabels`. Exported so tests seed and corrupt the real key.
 */
export const REMEMBERED_DAEMON_LABELS_STORAGE_KEY = "tddy_known_daemon_labels";

/**
 * The stored label map, or an empty one when nothing is stored, the stored value is not a
 * `RememberedDaemonLabels`, or `sessionStorage` cannot be read.
 *
 * A value this module cannot read names no daemon, so it is treated as no memory at all rather
 * than an error — and the next listing overwrites it. The only consequence is a waiting daemon
 * shown under its instance id. Both are deliberate, consented exceptions to "no fallbacks": the
 * memory is a display nicety, and failing the selector over it would be worse than forgetting.
 */
function readRememberedDaemonLabels(): Record<string, string> {
  let raw: string | null;
  try {
    raw = sessionStorage.getItem(REMEMBERED_DAEMON_LABELS_STORAGE_KEY);
  } catch {
    return {};
  }
  if (raw === null) return {};
  try {
    return fromJsonString(RememberedDaemonLabelsSchema, raw).labels;
  } catch {
    return {};
  }
}

/**
 * Remember the label every daemon in `daemons` is listed under, for this browser tab.
 *
 * Merges into what is already remembered, so a daemon that has dropped out of the list keeps its
 * label — that is exactly when the label is needed, since a listed daemon shows its own. A daemon
 * that re-advertises under another label is remembered under the newer one. An empty listing adds
 * nothing, so nothing is written; nor is anything when `sessionStorage` refuses the write — the
 * memory is then simply not kept.
 */
export function rememberDaemonLabels(daemons: readonly DaemonHost[]): void {
  if (daemons.length === 0) return;
  const labels = readRememberedDaemonLabels();
  for (const daemon of daemons) labels[daemon.instanceId] = daemon.label;
  const json = toJsonString(
    RememberedDaemonLabelsSchema,
    create(RememberedDaemonLabelsSchema, { labels }),
  );
  try {
    sessionStorage.setItem(REMEMBERED_DAEMON_LABELS_STORAGE_KEY, json);
  } catch {
    // Consented: a tab that cannot store the memory just does without it.
  }
}

/** The label this tab last saw `instanceId` listed under, or `null` if it has never been listed. */
export function readRememberedDaemonLabel(instanceId: string): string | null {
  const labels = readRememberedDaemonLabels();
  // Own keys only: an instance id such as `toString` must not read the object's prototype.
  return Object.hasOwn(labels, instanceId) ? labels[instanceId] : null;
}
