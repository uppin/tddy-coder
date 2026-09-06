/**
 * A terminal on a room this page was handed the coordinates for.
 *
 * The standalone connect screen (`src/index.tsx`, and the Storybook stories the `ghostty-*` E2E
 * suite drives) is not a session on a host: an operator types a LiveKit url, a room and an identity,
 * or a harness passes a pre-minted token in the query string, and the terminal on the other end is
 * whatever process is publishing into that room. There is no `SessionConnection` to open, because
 * there is no session and no daemon to ask about one.
 *
 * What that screen needs is exactly what the LiveKit terminal used to do for itself — connect a
 * `Room`, renew the token, wait for the server participant — with the terminal component no longer
 * doing any of it. So it lives here, beside the room-carried feed it produces, and the component is
 * handed a {@link TerminalFeed} like every other caller.
 *
 * No history: scrollback comes from a host daemon's capture ring, and this screen knows of no host
 * and no session id to ask about. The terminal degrades to live tail, which is what this screen has
 * always done.
 */

import { createClient } from "@connectrpc/connect";
import { DisconnectReason, Room, RoomEvent } from "livekit-client";
import { useEffect, useRef, useState } from "react";
import { TerminalService } from "../../../gen/terminal_pb";
import { tddyDebug } from "../../../lib/debugMask";
import { isCancelledLiveKitConnectionError } from "../../../lib/liveKitConnectionErrors";
import type { LiveKitChromeStatus } from "../../../lib/liveKitStatusPresentation";
import { useLiveKitTransportFactory } from "../../transportProvider";
import type { TerminalFeed } from "../terminal";
import { openRoomTerminalStream } from "./roomTerminalFeed";

const dRoom = tddyDebug("tddy:term:room");

/** How long before a token lapses its replacement is fetched — the LiveKit terminal's minute. */
const TOKEN_REFRESH_LEAD_MS = 60 * 1000;

export interface DirectRoomTerminalDeps {
  /** The LiveKit server to join. */
  readonly url: string;

  /**
   * A token to join with. Where {@link getToken} is also given, this is the one the first connect
   * uses and `getToken` renews it; where it is not, `getToken` mints the first as well.
   */
  readonly token?: string;

  /** Mints (and renews) a token for this room. */
  readonly getToken?: () => Promise<{ token: string; ttlSeconds: bigint }>;

  /** The life of {@link token}, so the first renewal can be scheduled. Required alongside both. */
  readonly ttlSeconds?: bigint;

  /** The participant serving `StreamTerminalIO` on the room. */
  readonly serverIdentity?: string;

  /** Named in diagnostics only. */
  readonly roomName?: string;

  /** Log the transport's own frames. */
  readonly debug?: boolean;
}

export interface DirectRoomTerminal {
  /** The terminal to render, once the room is up. `null` until then. */
  readonly feed: TerminalFeed | null;
  readonly status: LiveKitChromeStatus;
  /** Why the room is unusable, when {@link status} is `error`; `null` otherwise. */
  readonly error: string | null;
}

/**
 * Join `url`'s room and open the terminal the participant on it serves.
 *
 * The room is released when the screen unmounts or its coordinates change: a join left behind is a
 * participant the room keeps counting, and a renewal timer left behind re-arms itself for the life
 * of the page.
 */
export function useDirectRoomTerminal({
  url,
  token,
  getToken,
  ttlSeconds,
  serverIdentity = "server",
  roomName,
  debug = false,
}: DirectRoomTerminalDeps): DirectRoomTerminal {
  const transportFor = useLiveKitTransportFactory();
  const [feed, setFeed] = useState<TerminalFeed | null>(null);
  const [status, setStatus] = useState<LiveKitChromeStatus>("connecting");
  const [error, setError] = useState<string | null>(null);
  // The token the room is currently on, so a renewal and a reconnect use the same one.
  const heldToken = useRef<string | null>(null);

  useEffect(() => {
    if (!url || (!token && !getToken)) {
      setFeed(null);
      setStatus("connecting");
      return;
    }

    let cancelled = false;
    let refreshTimer: ReturnType<typeof setTimeout> | null = null;

    // The room object exists before the join does, and the terminal is opened against it at once —
    // the same shape `LiveKitSessionConnection` takes, and for the same reason: a screen that had to
    // wait for a media server before it could render its terminal would show the operator no
    // chrome, no keyboard and no way back out while the join was in flight.
    const room = new Room();
    const terminal = createClient(TerminalService, transportFor(room, serverIdentity, { debug }));
    // The stream waits for the server participant itself, so there is nothing to await here.
    const { stream, ended } = openRoomTerminalStream({
      room,
      serverIdentity,
      terminal,
      sessionId: roomName ?? "",
    });
    setFeed({ stream, ended });

    const scheduleRefresh = (life: bigint) => {
      if (!getToken) return;
      refreshTimer = setTimeout(
        () => {
          void getToken()
            .then((next) => {
              if (cancelled) return;
              heldToken.current = next.token;
              scheduleRefresh(next.ttlSeconds);
            })
            .catch((e) => {
              // The room carries on with the token it has; saying nothing is what would turn "the
              // renewal stopped" into "the terminal died for no reason".
              console.warn("[useDirectRoomTerminal] token refresh failed:", e);
            });
        },
        Math.max(0, Number(life) * 1000 - TOKEN_REFRESH_LEAD_MS),
      );
    };

    void (async () => {
      try {
        let initialToken: string;
        if (token && getToken && ttlSeconds !== undefined) {
          initialToken = token;
          scheduleRefresh(ttlSeconds);
        } else if (getToken) {
          const minted = await getToken();
          if (cancelled) return;
          initialToken = minted.token;
          scheduleRefresh(minted.ttlSeconds);
        } else if (token) {
          initialToken = token;
        } else {
          throw new Error("useDirectRoomTerminal: either token or getToken must be given");
        }
        heldToken.current = initialToken;

        room.on(RoomEvent.Disconnected, (reason) => {
          if (cancelled) return;
          // A renewable connection reconnects: the drop may be a network blip, and the token in hand
          // is still good. One that cannot renew has to say what happened instead.
          if (getToken && heldToken.current) {
            setStatus("connecting");
            void room.connect(url, heldToken.current).catch((e) => {
              console.warn("[useDirectRoomTerminal] reconnect failed:", e);
            });
            return;
          }
          const why = reason ?? DisconnectReason.UNKNOWN_REASON;
          if (why === DisconnectReason.CLIENT_INITIATED) return;
          setError(
            why === DisconnectReason.DUPLICATE_IDENTITY
              ? "Disconnected: another client joined with the same identity."
              : `LiveKit disconnected (reason=${why}).`,
          );
          setStatus("error");
        });
        room.on(RoomEvent.Connected, () => {
          if (cancelled) return;
          setStatus("connected");
        });

        dRoom("connecting url=%s room=%s serverIdentity=%s", url, roomName ?? "", serverIdentity);
        await room.connect(url, initialToken);
        if (cancelled) return;
        setStatus("connected");
      } catch (e) {
        if (cancelled) return;
        // A cancelled connect is this effect unwinding, not a failure to report.
        if (isCancelledLiveKitConnectionError(e)) return;
        setError(e instanceof Error ? e.message : String(e));
        setStatus("error");
      }
    })();

    return () => {
      cancelled = true;
      if (refreshTimer) clearTimeout(refreshTimer);
      stream.close();
      setFeed(null);
      room.disconnect();
    };
  }, [url, token, getToken, ttlSeconds, serverIdentity, roomName, debug, transportFor]);

  return { feed, status, error };
}
