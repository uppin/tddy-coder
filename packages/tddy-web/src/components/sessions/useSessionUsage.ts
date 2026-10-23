/**
 * Owns the `TddyRemote.Stream` bidirectional RPC for the Session Inspector "Usage" tab and keeps
 * the latest per-conversation token-usage snapshot.
 *
 * The presenter broadcasts the full cumulative snapshot as a `tokenUsageUpdated` `ServerMessage`
 * (never a delta); each event replaces the held snapshot. Everything else on the stream is
 * ignored here — this hook is a read-only viewer that only enqueues the eager open frame.
 *
 * The transport-selection and stream-open mechanics mirror `usePresenterChat`: a client is only
 * built when there is a genuinely connected `Room`, or when a test double has overridden the
 * LiveKit transport factory (those are free to ignore `room` and route to an in-memory backend).
 * Without one there is no stream to fail — a host reached without LiveKit has no usage stream at
 * all, and a room still joining has not opened one yet — so nothing is reported.
 *
 * Changeset: `session-usage-inspector`
 * PRD: `docs/ft/web/session-usage-inspector.md`
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { ConnectionState, RoomEvent, type Room } from "livekit-client";
import { AsyncQueue } from "tddy-livekit-web";
import {
  useLiveKitTransportFactory,
  useLiveKitTransportFactoryIsOverridden,
} from "../../rpc/transportProvider";
import {
  ClientMessageSchema,
  TddyRemote,
  type ClientMessage,
} from "../../gen/tddy/v1/remote_pb";
import { emptyUsage, type ConversationUsage } from "./sessionUsage";
import { rpcFailureText } from "../../rpc/rpcFailure";

/** The latest snapshot, and why the stream is not delivering (`null` while it is). */
export interface SessionUsageStream {
  usage: ConversationUsage[];
  /**
   * Why the stream stopped, ready to render; the last snapshot stays in `usage`. Cleared when a
   * snapshot arrives on a new subscription. `null` while there is no stream to fail.
   */
  error: string | null;
}

/** Whether `room` has joined, kept current across its connection-state changes. */
function useRoomConnected(room: Room | null): boolean {
  const [connected, setConnected] = useState(() => room?.state === ConnectionState.Connected);
  useEffect(() => {
    if (!room) {
      setConnected(false);
      return;
    }
    const sync = () => setConnected(room.state === ConnectionState.Connected);
    sync();
    room.on(RoomEvent.ConnectionStateChanged, sync);
    return () => {
      room.off(RoomEvent.ConnectionStateChanged, sync);
    };
  }, [room]);
  return connected;
}

/**
 * Subscribes to the session's token-usage stream and returns the latest snapshot, one entry per
 * conversation. `room` / `serverIdentity` select the LiveKit transport target exactly as they do
 * for the presenter chat. `usage` is `emptyUsage()` until the first `tokenUsageUpdated` event
 * arrives (or forever, if the session never reports usage); a stream that fails is reported in
 * `error` instead of looking like a session that reported nothing.
 */
export function useSessionUsage(
  room: Room | null,
  serverIdentity: string,
): SessionUsageStream {
  const liveKitFactory = useLiveKitTransportFactory();
  const factoryIsOverridden = useLiveKitTransportFactoryIsOverridden();
  const roomConnected = useRoomConnected(room);
  const canBuildClient = roomConnected || factoryIsOverridden;

  // Stable client for the lifetime of this room/identity pair — a new room or identity tears
  // down the old stream and opens a fresh one. `null` when there is no room yet and no test
  // double to route through instead.
  const client = useMemo(() => {
    if (!canBuildClient) return null;
    const transport = liveKitFactory(room as Room, serverIdentity);
    return createClient(TddyRemote, transport);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [liveKitFactory, canBuildClient, room, serverIdentity]);

  const [usage, setUsage] = useState<ConversationUsage[]>(emptyUsage);
  const [error, setError] = useState<string | null>(null);
  const queueRef = useRef<AsyncQueue<ClientMessage> | null>(null);

  useEffect(() => {
    setUsage(emptyUsage());
    setError(null);
    if (!client) {
      queueRef.current = null;
      return;
    }

    const queue = new AsyncQueue<ClientMessage>();
    queueRef.current = queue;
    let cancelled = false;

    // Open the stream eagerly. The LiveKit transport only publishes the stream-open frame (which
    // makes the server run `connect_view` -> replay snapshot + subscribe to live events) on the
    // first enqueued client message. An empty `ClientMessage` (no intent) triggers the open and is
    // ignored server-side.
    queue.enqueue(create(ClientMessageSchema, {}));

    (async () => {
      try {
        for await (const serverMessage of client.stream(queue)) {
          if (cancelled) break;
          if (serverMessage.event.case === "tokenUsageUpdated") {
            setError(null);
            setUsage(
              serverMessage.event.value.conversations.map((c) => ({
                agent: c.agent,
                id: c.id,
                model: c.model,
                inputTokens: c.inputTokens,
                outputTokens: c.outputTokens,
                totalTokens: c.totalTokens,
                turns: c.turns,
              })),
            );
          }
        }
      } catch (err) {
        // The last snapshot stays shown; the tab says it is no longer live.
        if (!cancelled) setError(rpcFailureText(err));
      }
    })();

    return () => {
      cancelled = true;
      queue.close();
      if (queueRef.current === queue) {
        queueRef.current = null;
      }
    };
  }, [client]);

  return { usage, error };
}
