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

/** How `tddy-rpc`'s router words a call to a service this daemon never registered. */
const UNKNOWN_SERVICE = /^Unknown service: (\S+)$/;

/**
 * The service a call reached a daemon that does not serve, or `null` for any other failure. A
 * daemon registers some services only when their config is set (accounts need `auth_storage`), so
 * this is a host's configuration speaking, not a fault in the call.
 */
export function unservedService(error: unknown): string | null {
  const failure = ConnectError.from(error);
  if (failure.code !== Code.NotFound) return null;
  return UNKNOWN_SERVICE.exec(failure.rawMessage)?.[1] ?? null;
}

/** The text to show for a failed call: the daemon's own words, without the transport's `[code]`. */
export function rpcFailureText(error: unknown): string {
  const service = unservedService(error);
  if (service) {
    return `This host's daemon does not serve ${service} — the service is not enabled in its config.`;
  }
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

/** {@link hostNotConnected}, already said: for a surface that holds the text rather than the error. */
export function hostNotConnectedText(host: string | null | undefined): string {
  return rpcFailureText(hostNotConnected(host));
}

/**
 * An action's failure with the action named — `Delete failed: <why>` — as an error a failure slot
 * can `report`. The reason is converted once, here, rather than said into a string and then wrapped
 * back into an error at every call site.
 */
export function actionFailed(action: string, error: unknown): Error {
  return new Error(`${action} failed: ${rpcFailureText(error)}`);
}
