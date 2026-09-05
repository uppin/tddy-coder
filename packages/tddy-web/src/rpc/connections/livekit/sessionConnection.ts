/**
 * A session reached over its own LiveKit room.
 *
 * Everything attaching to a LiveKit-carried session used to require, gathered into one object:
 *
 *   • the **second room join**, per attached session, that `useSessionLiveKitRoom` performs;
 *   • the `web-traffic-*` **observer identity** that join is made under, minted once per room;
 *   • the **browser token** and its TTL refresh, from `useLiveKitTerminalToken` and `useCommonRoom`;
 *   • **participant targeting** — `daemon-<instance>-<session>`, from `sessionParticipantRpcClient`;
 *   • the **client identity guarantee** `SessionClientCache` gives, now keyed by the connection.
 *
 * All five were hooks, so all five ran on a component's render schedule and had to be re-derived by
 * anything that wanted a session's client. As a connection they have one lifetime, which is what
 * lets `close()` mean something: today the session room's lifetime is a `useEffect` cleanup, and
 * "switch session without leaking a room" is a property of where the hook happened to be mounted.
 *
 * This file is the only place in the app that names a room, an identity or a token for a session —
 * the boundary PRD acceptance criterion 2 draws.
 */

import { createClient, type Client, type Transport } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import { ConnectionState, type Room } from "livekit-client";
import type { TokenService } from "../../../gen/token_pb";
import { capabilitiesForHint } from "../sessionAttachment";
import type { SessionAttachmentHint, SessionConnection } from "../session";
import type { ConnectionCapability, ConnectionStatus } from "../types";

/**
 * How long before a token expires its replacement is fetched.
 *
 * The same minute `useCommonRoom` leaves itself, for the same reason: a refresh that lands after
 * expiry is a reconnect, not a refresh.
 */
const TOKEN_REFRESH_LEAD_MS = 60 * 1000;

/**
 * What opening a LiveKit session connection needs from the app around it.
 *
 * Passed in rather than reached for, because none of it is available where a connection is opened:
 * `openSession` is called from an event handler or an effect, and `useHttpClient` /
 * `useLiveKitTransportFactory` are hooks. Bundling them also gives a test one seam to drive the real
 * join through — including the case where it fails, which has no other observable surface.
 */
export interface LiveKitSessionSupport {
  /** Mints and refreshes the browser token this connection joins its room with. */
  readonly tokens: Client<typeof TokenService>;

  /** Builds a transport addressed at one participant on a joined room. */
  readonly transportFor: (room: Room, targetIdentity: string) => Transport;

  /**
   * Constructs the `Room` object to join.
   *
   * The same injection seam `useCommonRoom` takes as `roomFactory`, and for the same reason: it is
   * the only way to drive a join — or a failed one — without a live media server.
   */
  readonly newRoom: () => Room;
}

/** A hint that actually names a room. `openLiveKitSession` refuses anything else. */
export interface RoomBackedHint extends SessionAttachmentHint {
  readonly room: string;
  readonly url?: string;
}

/**
 * The identity a browser joins a session's room under.
 *
 * Random, and minted once per connection: two tabs watching one session are two participants, and a
 * room that saw the same identity twice would drop one of them. `inferParticipantRole` reads the
 * `web-` prefix, so the roster still shows this as a browser rather than as an unknown peer.
 */
function anObserverIdentity(): string {
  return `web-traffic-${Math.random().toString(36).slice(2, 10)}`;
}

/**
 * The participant a session serves its own RPC on.
 *
 * The daemon states it in the attach reply (`livekitServerIdentity`). Deriving it is the fallback
 * for a reply that did not — the shape `sessionParticipantRpcClient` has always built by hand.
 */
export function sessionRpcIdentity(hostId: string, sessionId: string): string {
  return `daemon-${hostId}-${sessionId}`;
}

/**
 * One session, on one room, addressed at one participant.
 *
 * The `Room` object exists from construction and the join runs behind it, so `clientFor` answers
 * from the first render rather than after an await. That is what keeps a client's identity as stable
 * as its routing: the room object a transport is built against does not change when the join lands,
 * so `useAcpReplay`'s effect is not re-keyed by the connection merely finishing connecting. What
 * *does* produce fresh clients is a genuinely different route — another session, or a re-attach —
 * because that is a different connection object.
 */
class LiveKitSessionConnection implements SessionConnection {
  readonly capabilities: ReadonlySet<ConnectionCapability>;

  /** The participant this connection's calls are addressed to. */
  private readonly targetIdentity: string;

  private readonly room: Room;
  private readonly clients = new Map<DescService, Client<DescService>>();
  private builtTransport: Transport | null = null;
  private refreshTimer: ReturnType<typeof setTimeout> | null = null;
  private closed = false;
  private failure: string | null = null;

  constructor(
    readonly hostId: string,
    private readonly hint: RoomBackedHint,
    private readonly support: LiveKitSessionSupport,
  ) {
    this.capabilities = capabilitiesForHint(hint);
    this.targetIdentity = hint.serverIdentity ?? sessionRpcIdentity(hostId, hint.sessionId);
    this.room = support.newRoom();
    void this.join(anObserverIdentity());
  }

  get sessionId(): string {
    return this.hint.sessionId;
  }

  /** The room this connection joined — see {@link liveKitRoomOf}. */
  get joinedRoom(): Room {
    return this.room;
  }

  /**
   * Read at the moment it is asked, never captured — a session process leaves its room without
   * anything re-resolving this connection.
   *
   * `connecting` covers both "the room is not up yet" and "the room is up and the session process is
   * not on it", exactly as `LiveKitHostConnection` treats an absent daemon: from a caller's side
   * both say the call has nowhere to land right now and may have somewhere in a moment. A join that
   * genuinely failed is the one thing that is an `error`, and unlike the host connection this one
   * can reach it — minting a token or connecting a room are operations with a verdict.
   */
  get status(): ConnectionStatus {
    if (this.closed) return "idle";
    if (this.failure !== null) return "error";
    if (this.room.state !== ConnectionState.Connected) return "connecting";
    return this.room.remoteParticipants.has(this.targetIdentity) ? "connected" : "connecting";
  }

  get error(): string | null {
    return this.closed ? null : this.failure;
  }

  transport(): Transport {
    this.refuseIfClosed();
    this.builtTransport ??= this.support.transportFor(this.room, this.targetIdentity);
    return this.builtTransport;
  }

  clientFor<S extends DescService>(service: S): Client<S> {
    this.refuseIfClosed();
    const cached = this.clients.get(service);
    if (cached) return cached as Client<S>;
    const built = createClient(service, this.transport());
    this.clients.set(service, built as Client<DescService>);
    return built;
  }

  /**
   * Release the room, the refresh timer and the right to issue calls.
   *
   * Idempotent, and safe while the join is still in flight: the flag is read again once the connect
   * resolves, so a session detached mid-handshake does not end up with a room nobody will ever
   * disconnect. This is the whole reason a session connection is closeable — the room's lifetime
   * used to be a `useEffect` cleanup, so releasing it depended on where the hook was mounted.
   */
  close(): void {
    if (this.closed) return;
    this.closed = true;
    this.clearRefresh();
    this.room.disconnect();
  }

  private refuseIfClosed(): void {
    if (this.closed) {
      throw new Error(`session ${this.sessionId} on host ${this.hostId} is closed`);
    }
  }

  private async join(identity: string): Promise<void> {
    const url = this.hint.url;
    if (!url) {
      // A room with nowhere to reach it is not a connection that might yet come up.
      this.failure = `session ${this.sessionId} named room ${this.hint.room} but no LiveKit url`;
      return;
    }
    try {
      // No `sessionToken` is passed: `token.TokenService` carries the field, so
      // `createAuthGateInterceptor` fills it with a request-time-fresh access token on the way out
      // (`src/rpc/authGateInterceptor.ts`). Reading one here would send a staler credential.
      const minted = await this.support.tokens.generateToken({ room: this.hint.room, identity });
      if (this.closed) return;
      this.scheduleRefresh(minted.ttlSeconds, identity);
      await this.room.connect(url, minted.token);
      if (this.closed) this.room.disconnect();
    } catch (e) {
      this.failure = e instanceof Error ? e.message : String(e);
      this.clearRefresh();
      this.room.disconnect();
    }
  }

  /**
   * Re-mint the token a minute before it lapses, and again a minute before its replacement does.
   *
   * A failed refresh does not tear the room down: LiveKit may well carry on with the session it
   * already has, and dropping a working terminal over a token the room has not asked for yet would
   * be the worse of the two outcomes.
   */
  private scheduleRefresh(ttlSeconds: bigint, identity: string): void {
    this.clearRefresh();
    const delayMs = Math.max(0, Number(ttlSeconds) * 1000 - TOKEN_REFRESH_LEAD_MS);
    this.refreshTimer = setTimeout(() => {
      void this.support.tokens
        .refreshToken({ room: this.hint.room, identity })
        .then((next) => {
          if (this.closed) return;
          this.scheduleRefresh(next.ttlSeconds, identity);
        })
        // Deliberately swallowed — see the note above: the room outlives a refused refresh.
        .catch(() => {});
    }, delayMs);
  }

  private clearRefresh(): void {
    if (this.refreshTimer === null) return;
    clearTimeout(this.refreshTimer);
    this.refreshTimer = null;
  }
}

/**
 * Open `hint`'s session on `hostId` over LiveKit.
 *
 * The join starts immediately and the returned connection reports its progress through `status`;
 * there is nothing to await, because a caller that had to would be a caller unable to render until
 * a media server answered.
 */
export function openLiveKitSession(
  hostId: string,
  hint: RoomBackedHint,
  support: LiveKitSessionSupport,
): SessionConnection {
  return new LiveKitSessionConnection(hostId, hint, support);
}

/**
 * The LiveKit room behind `connection`, or `null` when it is not carried over one.
 *
 * The one thing a caller can legitimately want that the wire-neutral interface cannot express: the
 * traffic strip reads round-trip time off a room's own peer connection (`readRoomRtt`), which is a
 * measurement of the wire and not of the session. It lives here, next to the join, so that asking
 * for a room still means importing LiveKit — rather than widening `SessionConnection` with a member
 * every other transport would have to answer `null` to.
 *
 * Before this, the traffic strip joined the session's room a *second* time purely to measure it
 * (`useSessionLiveKitRoom`). Reading the connection's own room is the same measurement over one
 * fewer participant.
 */
export function liveKitRoomOf(connection: SessionConnection | null): Room | null {
  return connection instanceof LiveKitSessionConnection ? connection.joinedRoom : null;
}
