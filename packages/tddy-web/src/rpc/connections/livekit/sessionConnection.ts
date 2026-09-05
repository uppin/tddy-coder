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
import { ConnectionService } from "../../../gen/connection_pb";
import { TerminalService } from "../../../gen/terminal_pb";
import type { TokenService } from "../../../gen/token_pb";
import { capabilitiesForHint } from "../sessionAttachment";
import type { SessionAttachmentHint, SessionConnection } from "../session";
import type { TerminalFeed, TerminalOptions } from "../terminal";
import type { ConnectionCapability, ConnectionStatus } from "../types";
import { openRoomTerminalFeed } from "./roomTerminalFeed";

/**
 * When a token's replacement is fetched, and what happens when that fetch is refused.
 *
 * Every number here is a schedule rather than a policy decision a caller makes, so production never
 * states one. It is an interface only because a test asserting *when* a refresh is due cannot wait a
 * real hour to see it, and one asserting a retry cannot wait a real ten seconds.
 */
export interface TokenRefreshPolicy {
  /**
   * How long before a token expires its replacement is fetched.
   *
   * The same minute `useCommonRoom` leaves itself, for the same reason: a refresh that lands after
   * expiry is a reconnect, not a refresh.
   */
  readonly leadMs: number;

  /**
   * The soonest a refresh is ever scheduled for.
   *
   * A daemon issuing short-lived tokens would otherwise put `ttl - lead` at or below zero, and since
   * every renewal carries the same TTL the success path would re-arm at zero for ever — an unbounded
   * loop at `setTimeout`'s 4ms clamp, aimed at the daemon's own `TokenService`. Renewing a 30-second
   * token every 5 seconds is wasteful; renewing it 250 times a second is an outage.
   */
  readonly minDelayMs: number;

  /** How soon a *refused* refresh is tried again. Shorter than a lead, because the token that was
   *  not replaced is already inside its last minute. */
  readonly retryDelayMs: number;

  /** How many consecutive refusals are retried before the connection stops asking. */
  readonly maxRetries: number;
}

export const DEFAULT_TOKEN_REFRESH_POLICY: TokenRefreshPolicy = {
  leadMs: 60 * 1000,
  minDelayMs: 5 * 1000,
  retryDelayMs: 10 * 1000,
  maxRetries: 5,
};

/**
 * How long from now `ttlSeconds`'s replacement is due, under `policy`.
 *
 * Exported because it is the whole of the schedule and the one part of it a test can pin without
 * waiting: the floor is not a rounding detail but the thing standing between a short TTL and a spin
 * loop.
 */
export function tokenRefreshDelayMs(ttlSeconds: bigint, policy: TokenRefreshPolicy): number {
  return Math.max(policy.minDelayMs, Number(ttlSeconds) * 1000 - policy.leadMs);
}

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
   * A client for `service` on the **host** this session runs on, not on the session's own
   * participant.
   *
   * A session room answers `terminal.TerminalService/StreamTerminalIO` and nothing else, so the one
   * thing a session cannot ask its own room for is its scrollback: the capture ring belongs to the
   * host daemon. Reaching past the room for that — and only for that — is what gives a
   * LiveKit-carried session history it has never had, without moving any output byte off the room.
   */
  readonly hostClientFor: <S extends DescService>(service: S) => Client<S>;

  /**
   * Constructs the `Room` object to join.
   *
   * The same injection seam `useCommonRoom` takes as `roomFactory`, and for the same reason: it is
   * the only way to drive a join — or a failed one — without a live media server.
   */
  readonly newRoom: () => Room;

  /** Overrides for {@link DEFAULT_TOKEN_REFRESH_POLICY}. Production passes none. */
  readonly refreshPolicy?: Partial<TokenRefreshPolicy>;
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
  private readonly policy: TokenRefreshPolicy;
  private readonly clients = new Map<DescService, Client<DescService>>();
  private builtTransport: Transport | null = null;
  private refreshTimer: ReturnType<typeof setTimeout> | null = null;
  private refusedRefreshes = 0;
  private closed = false;
  /** True from construction until `join()` settles — see {@link close}. */
  private joining = true;
  private roomReleased = false;
  private failure: string | null = null;

  constructor(
    readonly hostId: string,
    private readonly hint: RoomBackedHint,
    private readonly support: LiveKitSessionSupport,
  ) {
    this.capabilities = capabilitiesForHint(hint);
    this.targetIdentity = hint.serverIdentity ?? sessionRpcIdentity(hostId, hint.sessionId);
    this.policy = { ...DEFAULT_TOKEN_REFRESH_POLICY, ...support.refreshPolicy };
    this.room = support.newRoom();
    void this.join(anObserverIdentity());
  }

  get sessionId(): string {
    return this.hint.sessionId;
  }

  /**
   * The room this connection joined, or `null` once it has been released — see
   * {@link liveKitRoomOf}.
   *
   * `null` after `close()` for the same reason `clientFor` throws there: a released room is a room
   * whose peer connection is going away, and a caller measuring it (`StatusBar`'s round-trip
   * readout) would be reading a wire that no longer exists.
   */
  get joinedRoom(): Room | null {
    return this.closed ? null : this.room;
  }

  /**
   * Read at the moment it is asked, never captured — a room's state changes without anything
   * re-resolving this connection.
   *
   * This is **reachability**, and deliberately not "the session process is on the roster". The
   * handshake overlay is driven off it, and the overlay is `pointer-events-auto` over the whole
   * pane: a status that waited for `remoteParticipants.has(targetIdentity)` could leave an
   * interactive terminal permanently covered by an un-dismissable sheet whenever the session
   * process published under an identity this connection did not predict, or left and rejoined the
   * room. An absent peer makes a *call* fail, with an error the caller can see and retry; a stuck
   * overlay has no recovery at all, so the two are not traded off against each other. The target
   * identity still routes every call — see {@link transport}.
   *
   * A join that genuinely failed is the one thing that is an `error`, and unlike a host connection
   * this one can reach it — minting a token or connecting a room are operations with a verdict.
   */
  get status(): ConnectionStatus {
    if (this.closed) return "idle";
    if (this.failure !== null) return "error";
    return this.room.state === ConnectionState.Connected ? "connected" : "connecting";
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
   * The terminal over this session's own room, with the host's scrollback behind it.
   *
   * The two halves come from different places on purpose: bytes from `StreamTerminalIO` on the
   * session participant, which is the only thing that participant serves, and history from the
   * host's `GetTerminalHistory`, which is the only place it exists. Before this, a LiveKit-carried
   * session had the first and simply went without the second.
   */
  openTerminal(options: TerminalOptions): TerminalFeed {
    this.refuseIfClosed();
    return openRoomTerminalFeed({
      room: this.room,
      serverIdentity: this.targetIdentity,
      terminal: this.clientFor(TerminalService),
      host: this.support.hostClientFor(ConnectionService),
      sessionId: this.sessionId,
      options,
    });
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
    // While the join is still in flight the *join* releases the room, not this. Disconnecting here
    // would hit a room that has not connected yet — a no-op — and the connect landing a moment
    // later would then leave a joined room nobody ever disconnects. Where the join has settled,
    // releasing here is the only chance there is.
    if (!this.joining) this.releaseRoom();
  }

  /**
   * Disconnect the room, at most once for the life of this connection.
   *
   * Both `close()` and the join's own unwinding can arrive here, and LiveKit's `disconnect()` is not
   * free to call twice: a second one races the reconnect logic of a room that is on its way down.
   */
  private releaseRoom(): void {
    if (this.roomReleased) return;
    this.roomReleased = true;
    this.room.disconnect();
  }

  private refuseIfClosed(): void {
    if (this.closed) {
      throw new Error(`session ${this.sessionId} on host ${this.hostId} is closed`);
    }
  }

  private async join(identity: string): Promise<void> {
    try {
      const url = this.hint.url;
      if (!url) {
        // A room with nowhere to reach it is not a connection that might yet come up.
        this.failure = `session ${this.sessionId} named room ${this.hint.room} but no LiveKit url`;
        return;
      }
      // No `sessionToken` is passed: `token.TokenService` carries the field, so
      // `createAuthGateInterceptor` fills it with a request-time-fresh access token on the way out
      // (`src/rpc/authGateInterceptor.ts`). Reading one here would send a staler credential.
      const minted = await this.support.tokens.generateToken({ room: this.hint.room, identity });
      if (this.closed) return;
      this.scheduleRefresh(minted.ttlSeconds, identity);
      await this.room.connect(url, minted.token);
    } catch (e) {
      this.failure = e instanceof Error ? e.message : String(e);
      this.clearRefresh();
      this.releaseRoom();
    } finally {
      // A `close()` that arrived while any of the above was in flight left the room to this: it is
      // only here that there is certainly nothing further to connect.
      this.joining = false;
      if (this.closed) this.releaseRoom();
    }
  }

  /**
   * Re-mint the token a minute before it lapses, and again a minute before its replacement does.
   *
   * A refused refresh does not tear the room down: LiveKit may well carry on with the session it
   * already has, and dropping a working terminal over a token the room has not asked for yet would
   * be the worse of the two outcomes. It does re-arm, though — a connection that stopped asking
   * after one transient refusal would go on working until the token lapsed and then drop the room
   * with nothing anywhere saying why.
   */
  private scheduleRefresh(ttlSeconds: bigint, identity: string): void {
    this.armRefresh(tokenRefreshDelayMs(ttlSeconds, this.policy), identity);
  }

  private armRefresh(delayMs: number, identity: string): void {
    this.clearRefresh();
    this.refreshTimer = setTimeout(() => {
      void this.support.tokens
        .refreshToken({ room: this.hint.room, identity })
        .then((next) => {
          if (this.closed) return;
          this.refusedRefreshes = 0;
          this.scheduleRefresh(next.ttlSeconds, identity);
        })
        .catch((e) => {
          if (this.closed) return;
          this.refusedRefreshes += 1;
          if (this.refusedRefreshes <= this.policy.maxRetries) {
            this.armRefresh(this.policy.retryDelayMs, identity);
            return;
          }
          // Out of retries. The room is left alone deliberately, but silence here is what would
          // turn "the token stopped being renewed" into "the terminal died for no reason".
          console.warn(
            `[sessionConnection] session ${this.sessionId} on host ${this.hostId} gave up ` +
              `renewing its LiveKit token after ${this.refusedRefreshes} refusals; the room will ` +
              `drop when the current token expires`,
            e,
          );
        });
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
 *
 * A **closed** connection answers `null` too. `StatusBar` reads this straight off the attachment,
 * and an attachment can outlive the connection it names by a render — handing it a room that is on
 * its way down would have it measuring a wire nobody is on.
 */
export function liveKitRoomOf(connection: SessionConnection | null): Room | null {
  return connection instanceof LiveKitSessionConnection ? connection.joinedRoom : null;
}
