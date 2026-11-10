/**
 * What a failed screen-sharing start says to the operator.
 *
 * The daemon waits for a desktop's bridge to say whether the desktop came up, and fails the start
 * with the reason it was given (`#host-desktops` 6/8). A refused sign-in arrives as
 * `Unauthenticated`, and is said as one, with the desktop's own words after it: the operator has to
 * know it was the password, not that the desktop could not be reached. Any other failure is the
 * daemon's reason, without the transport's status-code prefix.
 */

import { Code, ConnectError } from "@connectrpc/connect";

export function screenSharingStartFailureText(error: unknown): string {
  if (error instanceof ConnectError) {
    return error.code === Code.Unauthenticated
      ? `The desktop refused the sign-in: ${error.rawMessage}`
      : error.rawMessage;
  }
  return error instanceof Error ? error.message : String(error);
}
