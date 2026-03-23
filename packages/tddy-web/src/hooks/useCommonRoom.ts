import { useEffect, useMemo, useRef, useState } from "react";
import { Room } from "livekit-client";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import { TokenService } from "../gen/token_pb";

export type CommonRoomStatus = "idle" | "connecting" | "connected" | "error";

function createTokenClient() {
  const transport = createConnectTransport({
    baseUrl: typeof window !== "undefined" ? `${window.location.origin}/rpc` : "",
    useBinaryFormat: true,
  });
  return createClient(TokenService, transport);
}

/**
 * Joins a shared LiveKit room (presence / participant list). Disconnects on unmount or when inputs change.
 */
export function useCommonRoom(
  url: string | undefined,
  roomName: string | undefined,
  identity: string | undefined
): { room: Room | null; status: CommonRoomStatus; error: string | null } {
  const [room, setRoom] = useState<Room | null>(null);
  const [status, setStatus] = useState<CommonRoomStatus>("idle");
  const [error, setError] = useState<string | null>(null);
  const tokenClient = useMemo(() => createTokenClient(), []);
  const refreshTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const roomRef = useRef<Room | null>(null);

  useEffect(() => {
    const u = url?.trim();
    const rn = roomName?.trim();
    const id = identity?.trim();
    if (!u || !rn || !id) {
      setStatus("idle");
      setError(null);
      setRoom(null);
      return;
    }

    let cancelled = false;

    const clearRefresh = () => {
      if (refreshTimerRef.current) {
        clearTimeout(refreshTimerRef.current);
        refreshTimerRef.current = null;
      }
    };

    const run = async () => {
      setStatus("connecting");
      setError(null);
      setRoom(null);
      roomRef.current?.disconnect();
      roomRef.current = null;

      try {
        const res = await tokenClient.generateToken({ room: rn, identity: id });
        if (cancelled) return;

        const ttlMs = Number(res.ttlSeconds) * 1000;
        const refreshInMs = Math.max(0, ttlMs - 60 * 1000);
        const scheduleRefresh = (delayMs: number) => {
          clearRefresh();
          refreshTimerRef.current = setTimeout(async () => {
            try {
              const next = await tokenClient.refreshToken({ room: rn, identity: id });
              const nextTtlMs = Number(next.ttlSeconds) * 1000;
              scheduleRefresh(Math.max(0, nextTtlMs - 60 * 1000));
            } catch {
              // Token refresh failure does not tear down the room; LiveKit may reconnect with existing session.
            }
          }, delayMs);
        };
        scheduleRefresh(refreshInMs);

        const liveRoom = new Room();
        roomRef.current = liveRoom;
        await liveRoom.connect(u, res.token);
        if (cancelled) {
          liveRoom.disconnect();
          roomRef.current = null;
          return;
        }
        setRoom(liveRoom);
        setStatus("connected");
      } catch (e) {
        if (cancelled) return;
        clearRefresh();
        setError(e instanceof Error ? e.message : String(e));
        setStatus("error");
        setRoom(null);
        roomRef.current?.disconnect();
        roomRef.current = null;
      }
    };

    void run();

    return () => {
      cancelled = true;
      clearRefresh();
      roomRef.current?.disconnect();
      roomRef.current = null;
      setRoom(null);
    };
  }, [url, roomName, identity, tokenClient]);

  return { room, status, error };
}
