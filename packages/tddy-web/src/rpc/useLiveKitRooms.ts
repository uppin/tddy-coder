/**
 * Streaming hook behind the LiveKit rooms panel: every room on the LiveKit server and the
 * participants joined to each.
 *
 * Sourced from a single `ConnectionService.StreamLiveKitRooms` server-stream over the shared
 * common-room LiveKit connection (`useDaemonClient`), so the panel follows the daemon selector like
 * every other daemon-level readout. The daemon owns the cadence and the feed's shape: its first
 * message is a full snapshot, every message after it one change event. The fold itself lives in
 * `lib/liveKitRoomsState` so the arithmetic is unit-testable.
 *
 * PRD: `docs/ft/web/livekit-rooms-panel.md`
 * Changeset: `livekit-rooms-panel`
 */

import { useEffect, useState } from "react";
import { ConnectError } from "@connectrpc/connect";
import { ConnectionService } from "../gen/connection_pb";
import { useDaemonClient } from "./selectedDaemon";
import { useAuthContext } from "../hooks/authProvider";
import {
  applyRoomsChange,
  roomsFromSnapshot,
  type LiveKitRoom,
} from "../lib/liveKitRoomsState";

export interface UseLiveKitRoomsResult {
  /** Rooms as of the last message applied. Empty until the first snapshot arrives. */
  rooms: LiveKitRoom[];
  /** Whether the feed's first message has arrived, which is what tells "empty" from "loading". */
  hasSnapshot: boolean;
  /** The reason the stream failed, or `null` while it is healthy. */
  error: string | null;
}

/**
 * Subscribe once to `ConnectionService.StreamLiveKitRooms` for the selected daemon and expose the
 * folded room list.
 *
 * Cleanup aborts the call rather than only flagging the loop, which is what
 * `useTaskListStream`/`useTaskChannelStream` already do. This feed is deliberately silent while
 * nothing changes, so a flag alone would be observed only on the *next* event — on an idle server
 * that never comes, the iterator stays parked forever and the call is never settled. Aborting ends
 * it at unmount; the resulting AbortError is swallowed the way `useHostStats` swallows its own.
 *
 * The reach of that abort is client-side only: `packages/tddy-livekit-web/src/transport.ts` cancels
 * locally without telling the peer (a TODO carried there), so this stops the browser's loop and
 * releases its pending-call entry, while ending the daemon's side is a separate server-side fix.
 *
 * A stream that fails *after* a snapshot keeps the last-known rooms alongside the error: a stale
 * roster plus a visible error beats an empty panel.
 */
export function useLiveKitRooms(): UseLiveKitRoomsResult {
  const client = useDaemonClient(ConnectionService);
  const { sessionToken } = useAuthContext();
  const [rooms, setRooms] = useState<LiveKitRoom[]>([]);
  const [hasSnapshot, setHasSnapshot] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!client) return;
    const controller = new AbortController();
    // A new subscription starts over: the daemon's first frame is an authoritative snapshot.
    setRooms([]);
    setHasSnapshot(false);
    setError(null);

    (async () => {
      try {
        for await (const event of client.streamLiveKitRooms(
          { sessionToken: sessionToken ?? "" },
          { signal: controller.signal },
        )) {
          if (event.event.case === "snapshot") {
            setRooms(roomsFromSnapshot(event.event.value.rooms));
            setHasSnapshot(true);
          } else if (event.event.case === "change") {
            const change = event.event.value;
            setRooms((prev) => applyRoomsChange(prev, change));
          }
        }
      } catch (err) {
        // The abort this effect's cleanup raises is the unmount/re-subscribe path; ignore it. Any
        // other error while still subscribed is the daemon's own, and the panel shows it next to
        // whatever roster the last snapshot left behind (no fallback fabrication).
        if (!controller.signal.aborted) {
          setError(ConnectError.from(err).rawMessage);
        }
      }
    })();

    return () => {
      controller.abort();
    };
  }, [client, sessionToken]);

  return { rooms, hasSnapshot, error };
}
