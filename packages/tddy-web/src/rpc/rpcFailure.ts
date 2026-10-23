/**
 * What an operator is told when an RPC fails — the one vocabulary every screen's failure display
 * uses, so no call site is left to swallow an error because it had nothing to say.
 *
 * Two failures look alike to a screen and must not: the daemon refused (its reason is shown
 * verbatim), and nothing reaches the host at all (`useDaemonClient` answered `null`). The second
 * used to be an early `return` — a click that did nothing — so it is raised here as an error of its
 * own, with the host named.
 */

import { Code, ConnectError } from "@connectrpc/connect";

/** The text to show for a failed call: the daemon's own words, without the transport's `[code]`. */
export function rpcFailureText(error: unknown): string {
  const failure = ConnectError.from(error);
  const message = failure.rawMessage || String(error);
  return failure.code === Code.PermissionDenied ? `Permission denied — ${message}` : message;
}

/**
 * The failure an action raises when no connection reaches its host: there is no client to call, so
 * the daemon never heard the request. `host` is its label, or its instance id when it has none.
 */
export function hostNotConnected(host: string | null | undefined): ConnectError {
  return new ConnectError(
    `Not connected to ${host || "the selected host"} — it may be offline or not yet in the room.`,
    Code.Unavailable,
  );
}
