/**
 * Whether a file dropped from this machine's file manager onto a session's terminal can be handed
 * to the session as its real path, rather than uploaded.
 *
 * Two things must both hold. The page must be inside Tddy Desktop and the session must be on the
 * daemon that application hosts — the only daemon on *this* machine; a session on any other host
 * is on another filesystem. And the daemon must say the session's agent reads its host's
 * filesystem (`SessionEntry.readsHostPaths`): a jailed, managed-codebase, split or SSH-exec session
 * on this very machine still cannot open an arbitrary path under `/Users`.
 *
 * PRD: docs/ft/web/web-terminal.md § File drop upload
 */

import type { SessionEntry } from "../gen/session_pb";
import { daemonTransportFlavour, type TauriHostWindow } from "../rpc/daemonTransportFlavour";

export function sessionReadsLocalPaths(
  win: TauriHostWindow,
  session: Pick<SessionEntry, "daemonInstanceId" | "readsHostPaths"> | undefined,
  servingDaemonInstanceId: string | undefined,
): boolean {
  if (daemonTransportFlavour(win) !== "webview-ipc" || !session) return false;
  const thisMachinesDaemon = servingDaemonInstanceId?.trim() ?? "";
  return (
    thisMachinesDaemon !== "" &&
    session.daemonInstanceId === thisMachinesDaemon &&
    session.readsHostPaths
  );
}
