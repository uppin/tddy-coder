import { ConnectError } from "@connectrpc/connect";

/** The daemon's reason, verbatim — without the transport's `[code]` prefix. */
export function reasonOf(error: unknown): string {
  return ConnectError.from(error).rawMessage;
}
