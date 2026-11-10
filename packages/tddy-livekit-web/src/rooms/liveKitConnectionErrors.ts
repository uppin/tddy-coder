import { ConnectionError, ConnectionErrorReason } from "livekit-client";

/** True when LiveKit aborted connect (e.g. `room.disconnect()` during connect) — not a user-facing failure. */
export function isCancelledLiveKitConnectionError(e: unknown): boolean {
  return (
    e instanceof ConnectionError &&
    e.reason === ConnectionErrorReason.Cancelled
  );
}
