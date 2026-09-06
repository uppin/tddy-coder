import { useCallback, useEffect, useRef, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { TokenService } from "../../gen/token_pb";

type TokenClient = Client<typeof TokenService>;

export interface LiveKitTerminalToken {
  token: string | null;
  ttlSeconds: bigint | null;
  error: string | null;
  getToken: () => Promise<{ token: string; ttlSeconds: bigint }>;
}

/**
 * Fetches a browser LiveKit access token scoped to `roomName`/`identity`, for handing to
 * a caller that owns its own room join and its refresh scheduling (`getToken`/`ttlSeconds`).
 *
 * Does nothing (token/ttlSeconds/error stay null) until `tokenClient`, `roomName`, and `identity`
 * are all present and non-empty — e.g. before a session has finished attaching.
 *
 * The daemon's mint refuses an unauthenticated caller, but no `sessionToken` is passed here:
 * `token.TokenService` requests carry the field, so `createAuthGateInterceptor` fills it with a
 * request-time-fresh access token on the way out (see `src/rpc/authGateInterceptor.ts`) — fresher
 * than anything this hook could read, and without coupling it to an `AuthProvider`.
 */
export function useLiveKitTerminalToken(
  tokenClient: TokenClient | undefined,
  roomName: string | undefined,
  identity: string | undefined,
): LiveKitTerminalToken {
  const [token, setToken] = useState<string | null>(null);
  const [ttlSeconds, setTtlSeconds] = useState<bigint | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Latest values for getToken()'s closure, without forcing a new callback identity per render.
  const latest = useRef({ tokenClient, roomName, identity });
  latest.current = { tokenClient, roomName, identity };

  useEffect(() => {
    const room = roomName?.trim();
    const id = identity?.trim();
    if (!tokenClient || !room || !id) {
      setToken(null);
      setTtlSeconds(null);
      setError(null);
      return;
    }

    let cancelled = false;
    setError(null);

    tokenClient
      .generateToken({ room, identity: id })
      .then((res) => {
        if (cancelled) return;
        setToken(res.token);
        setTtlSeconds(res.ttlSeconds);
      })
      .catch((e) => {
        if (cancelled) return;
        setToken(null);
        setTtlSeconds(null);
        setError(e instanceof Error ? e.message : String(e));
      });

    return () => {
      cancelled = true;
    };
  }, [tokenClient, roomName, identity]);

  const getToken = useCallback(async () => {
    const { tokenClient: client, roomName: room, identity: id } = latest.current;
    const trimmedRoom = room?.trim();
    const trimmedIdentity = id?.trim();
    if (!client || !trimmedRoom || !trimmedIdentity) {
      throw new Error("useLiveKitTerminalToken: getToken() called before room/identity are known");
    }
    const res = await client.refreshToken({ room: trimmedRoom, identity: trimmedIdentity });
    return { token: res.token, ttlSeconds: res.ttlSeconds };
  }, []);

  return { token, ttlSeconds, error, getToken };
}
