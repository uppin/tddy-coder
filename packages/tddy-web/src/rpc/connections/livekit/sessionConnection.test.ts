/**
 * Unit tests for opening a session connection on a LiveKit host.
 *
 * Four beliefs the whole session surface rests on, and which used to be spread across four hooks
 * where nothing could assert them together:
 *
 *   • **Where a session's calls go.** `daemon-<instance>-<session>`, the participant the session's
 *     own process serves on — not the daemon participant, which is what a session RPC fell back to
 *     whenever the LiveKit path was not taken (`SessionRuntime.tsx:176`).
 *   • **A session with no room is an ordinary session.** It routes over the host itself and
 *     advertises `rpc`. Today that case is `connected-grpc`, a second path every consumer branches
 *     on and which never even shows a connection status.
 *   • **A client's identity is as stable as its routing.** `useAcpReplay` keys an effect on the
 *     client, so a needlessly fresh one cancels an in-flight snapshot pull.
 *   • **`close()` releases the room.** It used to be a `useEffect` cleanup, so whether switching
 *     sessions leaked a joined room was a property of where the hook happened to be mounted.
 *
 * Driven through `LiveKitConnectionProvider`, because `openSession`'s first decision — room or no
 * room — is made there, and a test that reached past it would not be testing that decision.
 *
 * Technical: `packages/tddy-web/docs/session-connections.md`
 */

import { describe, it, expect } from "bun:test";
import type { Client, Transport } from "@connectrpc/connect";
import { ConnectionState, type Room } from "livekit-client";
import { ConnectionService } from "../../../gen/connection_pb";
import type { TokenService } from "../../../gen/token_pb";
import { daemonRpcIdentity } from "../../../lib/participantRole";
import { LiveKitConnectionProvider, type LiveKitSessionResources } from "../liveKit";
import { attachmentHintFromReply } from "../sessionAttachment";
import type { HostConnection } from "../types";
import {
  DEFAULT_TOKEN_REFRESH_POLICY,
  liveKitRoomOf,
  tokenRefreshDelayMs,
  type TokenRefreshPolicy,
} from "./sessionConnection";

const A_HOST = "instance-a";
const A_SESSION = "session-0001";
const ANOTHER_SESSION = "session-0002";
const A_ROOM_URL = "wss://livekit.example";

/** What the daemon replies for a session it publishes into a room of its own. */
const A_ROOM_BACKED_REPLY = {
  livekitRoom: "daemon-session-0001",
  livekitUrl: A_ROOM_URL,
  livekitServerIdentity: `daemon-${A_HOST}-${A_SESSION}`,
};

/**
 * A session server identity nothing could have derived from the host and session ids.
 *
 * The default reply states exactly what `sessionRpcIdentity` builds, so a test using it cannot tell
 * an implementation that reads the reply from one that ignores it.
 */
const A_NAMED_SERVER_IDENTITY = "coder-worker-7";

/** What it replies for a session it serves itself — the shape a desktop app over IPC produces. */
const A_HOST_SERVED_REPLY = { livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" };

/**
 * A refresh schedule that falls due immediately, for the specs that need to watch one happen.
 *
 * Production's is `DEFAULT_TOKEN_REFRESH_POLICY` — a minute's lead and a five-second floor, which is
 * what {@link tokenRefreshDelayMs}'s own specs pin. Nothing here asserts the schedule; these use it
 * only to reach the timer's *behaviour* without waiting for a real one.
 */
const A_REFRESH_DUE_AT_ONCE: TokenRefreshPolicy = {
  leadMs: 0,
  minDelayMs: 1,
  retryDelayMs: 1,
  maxRetries: 5,
};

/**
 * Let every queued promise continuation and expired timer run.
 *
 * A `close()` that lands while the join is still in flight is completed by the join's own
 * continuation, so an assertion made in the same microtask sees only half of what happened — which
 * is exactly how a second `room.disconnect()` hid from these specs.
 */
const settled = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 0));

/**
 * Long enough for a schedule running on {@link A_REFRESH_DUE_AT_ONCE}'s single millisecond to fire
 * several more times if it were going to.
 *
 * The only place in this file that waits on a duration, because it is the only assertion about
 * something *not* happening — there is no event to synchronise on when nothing is scheduled.
 */
const aQuietMoment = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 25));

// ---------------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------------

/** A transport that routes nowhere — these tests assert routing, never traffic. */
function aTransportStub(): Transport {
  return {
    unary: () => {
      throw new Error("this transport is a routing stand-in and issues no calls");
    },
    stream: () => {
      throw new Error("this transport is a routing stand-in and issues no calls");
    },
  } as unknown as Transport;
}

/** A transport factory that remembers which room and participant each transport was built for. */
function aRecordingTransportFactory() {
  const routes: { room: Room; targetIdentity: string }[] = [];
  const factory = (room: Room, targetIdentity: string): Transport => {
    routes.push({ room, targetIdentity });
    return aTransportStub();
  };
  return { factory, routes };
}

/** The common room the provider itself is bound to, with `hostsOnIt` present as daemon peers. */
function aCommonRoomWith(hostsOnIt: string[]): Room {
  return {
    state: ConnectionState.Connected,
    remoteParticipants: new Map(
      hostsOnIt.map((id) => [daemonRpcIdentity(id), { identity: daemonRpcIdentity(id) }]),
    ),
  } as unknown as Room;
}

/**
 * A session room that records its join and its release rather than reaching a media server.
 *
 * `joined` and `released` settle the moment `connect` and `disconnect` are called, so a test
 * synchronises on the thing it is waiting for instead of guessing how long a mint takes.
 */
function aJoinableRoom() {
  let announceJoin: (join: { url: string; token: string }) => void = () => {};
  const joined = new Promise<{ url: string; token: string }>((resolve) => {
    announceJoin = resolve;
  });
  let announceRelease: () => void = () => {};
  const released = new Promise<void>((resolve) => {
    announceRelease = resolve;
  });
  let releases = 0;
  const room = {
    state: ConnectionState.Disconnected,
    remoteParticipants: new Map<string, { identity: string }>(),
    connect: async (url: string, token: string) => {
      room.state = ConnectionState.Connected;
      announceJoin({ url, token });
    },
    disconnect: async () => {
      room.state = ConnectionState.Disconnected;
      releases += 1;
      announceRelease();
    },
  };
  return {
    asRoom: room as unknown as Room,
    joined,
    released,
    releaseCount: () => releases,
    /** Put `identity` on the roster — what the session's own process does when it comes up. */
    admit: (identity: string) => room.remoteParticipants.set(identity, { identity }),
  };
}

/** A mint that names its tokens after the identity they were issued for, so a join is legible. */
function aTokenMint(ttlSeconds = 3600n): Client<typeof TokenService> {
  return {
    generateToken: async ({ identity }: { room: string; identity: string }) => ({
      token: `token-for-${identity}`,
      ttlSeconds,
    }),
    refreshToken: async ({ identity }: { room: string; identity: string }) => ({
      token: `refreshed-token-for-${identity}`,
      ttlSeconds,
    }),
  } as unknown as Client<typeof TokenService>;
}

/** A mint that refuses, and announces the refusal — the daemon rejecting an unauthorised browser. */
function aRefusingTokenMint(reason: string): Client<typeof TokenService> {
  return {
    generateToken: async () => {
      throw new Error(reason);
    },
    refreshToken: async () => {
      throw new Error(reason);
    },
  } as unknown as Client<typeof TokenService>;
}

/**
 * A mint that remembers who it issued for, and whose `refreshed` promise settles when the TTL timer
 * comes back for a second token.
 */
function aTokenMintAwaitingRefresh(ttlSeconds: bigint) {
  const joinedAs: string[] = [];
  let announceRefresh: (identity: string) => void = () => {};
  const refreshed = new Promise<string>((resolve) => {
    announceRefresh = resolve;
  });
  const tokens = {
    generateToken: async ({ identity }: { room: string; identity: string }) => {
      joinedAs.push(identity);
      return { token: `token-for-${identity}`, ttlSeconds };
    },
    refreshToken: async ({ identity }: { room: string; identity: string }) => {
      announceRefresh(identity);
      return { token: `refreshed-token-for-${identity}`, ttlSeconds };
    },
  } as unknown as Client<typeof TokenService>;
  return { tokens, refreshed, joinedAs: () => joinedAs };
}

/**
 * A mint that issues the first token and then refuses every renewal, counting how often it is asked.
 *
 * `askedTwice` settles on the *second* refusal — the retry — so a spec waits for the behaviour it
 * is about rather than for a duration.
 */
function aTokenMintRefusingRefresh(ttlSeconds: bigint) {
  let attempts = 0;
  let announceRetry: () => void = () => {};
  const askedTwice = new Promise<void>((resolve) => {
    announceRetry = resolve;
  });
  const tokens = {
    generateToken: async ({ identity }: { room: string; identity: string }) => ({
      token: `token-for-${identity}`,
      ttlSeconds,
    }),
    refreshToken: async () => {
      attempts += 1;
      if (attempts >= 2) announceRetry();
      throw new Error("the daemon refused to renew this token");
    },
  } as unknown as Client<typeof TokenService>;
  return { tokens, askedTwice, attempts: () => attempts };
}

/** The host connection under test, from a provider wired with `resources`. */
function aHostOn(
  factory: (room: Room, targetIdentity: string) => Transport,
  resources: LiveKitSessionResources | null,
): HostConnection {
  const provider = new LiveKitConnectionProvider(aCommonRoomWith([A_HOST]), factory, resources);
  const host = provider.connectHost(A_HOST);
  if (!host) throw new Error("a provider holding a room must claim every host asked of it");
  return host;
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("opening a session whose attach reply names a room", () => {
  it("addresses the participant the reply named", () => {
    // Given a daemon serving this session's RPC from a worker of its own naming — an identity
    // nothing could arrive at by deriving one
    const { factory, routes } = aRecordingTransportFactory();
    const session = aHostOn(factory, {
      tokens: aTokenMint(),
      newRoom: () => aJoinableRoom().asRoom,
    }).openSession(
      A_SESSION,
      attachmentHintFromReply(A_SESSION, {
        ...A_ROOM_BACKED_REPLY,
        livekitServerIdentity: A_NAMED_SERVER_IDENTITY,
      }),
    );

    // When it builds a client
    session.clientFor(ConnectionService);

    // Then the calls go where the daemon said — not to the daemon participant, which is where a
    // session RPC quietly landed whenever the LiveKit path was not taken
    expect(routes.map((r) => r.targetIdentity)).toEqual([A_NAMED_SERVER_IDENTITY]);
  });

  it("derives the session participant when the reply did not name one", () => {
    // Given a reply from a daemon old enough not to state the session's server identity
    const { factory, routes } = aRecordingTransportFactory();
    const session = aHostOn(factory, {
      tokens: aTokenMint(),
      newRoom: () => aJoinableRoom().asRoom,
    }).openSession(
      A_SESSION,
      attachmentHintFromReply(A_SESSION, { ...A_ROOM_BACKED_REPLY, livekitServerIdentity: "" }),
    );

    // When it builds a client
    session.clientFor(ConnectionService);

    // Then the identity is derived, exactly as `sessionParticipantRpcClient` has always built it
    expect(routes.map((r) => r.targetIdentity)).toEqual([`daemon-${A_HOST}-${A_SESSION}`]);
  });

  it("joins the named room with a token minted for a browser observer identity", async () => {
    // Given a session room waiting to be joined
    const room = aJoinableRoom();
    const { factory } = aRecordingTransportFactory();
    aHostOn(factory, { tokens: aTokenMint(), newRoom: () => room.asRoom }).openSession(
      A_SESSION,
      attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY),
    );

    // When the join lands
    const join = await room.joined;

    // Then it went to the url the reply named, under an identity the roster reads as a browser.
    // The suffix is random by design — two tabs on one session must not collide — so it is the one
    // thing here matched by shape rather than by value.
    expect(join.url).toEqual(A_ROOM_URL);
    expect(join.token).toMatch(/^token-for-web-traffic-[a-z0-9]+$/);
  });

  it("re-mints for the observer that joined, not for somebody new", async () => {
    // Given an attached session whose renewal falls due at once
    const mint = aTokenMintAwaitingRefresh(0n);
    const { factory } = aRecordingTransportFactory();
    const session = aHostOn(factory, {
      tokens: mint.tokens,
      newRoom: () => aJoinableRoom().asRoom,
      refreshPolicy: A_REFRESH_DUE_AT_ONCE,
    }).openSession(A_SESSION, attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY));

    // When the refresh timer comes round
    const refreshedFor = await mint.refreshed;

    // Then it renews the token of the very observer that is on the room. A renewal for anyone else
    // is not a renewal — it is a second participant, and the one holding the wire keeps the token
    // that is about to lapse
    expect(refreshedFor).toEqual(mint.joinedAs()[0]!);
    session.close();
  });

  it("asks again after a refused renewal, rather than never renewing again", async () => {
    // Given a daemon refusing to renew, and a session whose renewal falls due at once
    const mint = aTokenMintRefusingRefresh(0n);
    const { factory } = aRecordingTransportFactory();
    const session = aHostOn(factory, {
      tokens: mint.tokens,
      newRoom: () => aJoinableRoom().asRoom,
      refreshPolicy: A_REFRESH_DUE_AT_ONCE,
    }).openSession(A_SESSION, attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY));

    // When the first refusal comes back
    await mint.askedTwice;

    // Then it has already tried again. Leaving the room up through a refusal is deliberate —
    // LiveKit carries on with the token it has — but a connection that then stops asking works
    // until expiry and drops the room with nothing anywhere saying why
    expect(mint.attempts()).toBeGreaterThanOrEqual(2);
    session.close();
  });

  it("stops asking once its retries are spent", async () => {
    // Given a daemon that will never renew, and a schedule allowing exactly one retry
    const mint = aTokenMintRefusingRefresh(0n);
    const { factory } = aRecordingTransportFactory();
    const session = aHostOn(factory, {
      tokens: mint.tokens,
      newRoom: () => aJoinableRoom().asRoom,
      refreshPolicy: { ...A_REFRESH_DUE_AT_ONCE, maxRetries: 1 },
    }).openSession(A_SESSION, attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY));

    // When the retry has also been refused, and the schedule is given room to fire again
    await mint.askedTwice;
    await aQuietMoment();

    // Then it asked the once it was allowed to and no more — a retry that re-armed for ever would
    // be the spin loop it exists to avoid, aimed at a daemon that has already said no twice
    expect(mint.attempts()).toEqual(2);
    session.close();
  });

  it("reports an error when the daemon refuses to mint a token for the room", async () => {
    // Given a mint that turns the browser away
    const room = aJoinableRoom();
    const { factory } = aRecordingTransportFactory();
    const session = aHostOn(factory, {
      tokens: aRefusingTokenMint("browser is not authorised for this room"),
      newRoom: () => room.asRoom,
    }).openSession(A_SESSION, attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY));

    // When the refusal comes back — which the connection answers by letting the room go
    await room.released;

    // Then the session says so. A host connection cannot reach this state — a peer that is absent
    // may yet appear — but a mint that refused has given its verdict, and the handshake overlay is
    // the surface that has to show it.
    expect(session.status).toEqual("error");
    expect(session.error).toEqual("browser is not authorised for this room");
  });

  it("stays connecting until its room is up", () => {
    // Given a session whose room is still being joined — the mint has not even come back
    const room = aJoinableRoom();
    const { factory } = aRecordingTransportFactory();
    const session = aHostOn(factory, {
      tokens: aTokenMint(),
      newRoom: () => room.asRoom,
    }).openSession(A_SESSION, attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY));

    // Then a call has nowhere to land, which is what `connecting` says
    expect(session.status).toEqual("connecting");
  });

  it("reports connected once its room is up", async () => {
    // Given a session room being joined
    const room = aJoinableRoom();
    const { factory } = aRecordingTransportFactory();
    const session = aHostOn(factory, {
      tokens: aTokenMint(),
      newRoom: () => room.asRoom,
    }).openSession(A_SESSION, attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY));

    // When the join lands
    await room.joined;

    // Then the connection is usable, and the handshake overlay can come down
    expect(session.status).toEqual("connected");
  });

  it("does not wait for the session's own process to appear on the room", async () => {
    // Given a joined room carrying somebody, but not the participant this connection addresses —
    // a session process that published under an identity nobody predicted, or that left and has
    // not rejoined yet
    const room = aJoinableRoom();
    const { factory } = aRecordingTransportFactory();
    const session = aHostOn(factory, {
      tokens: aTokenMint(),
      newRoom: () => room.asRoom,
    }).openSession(A_SESSION, attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY));

    // When the join lands and the roster shows only strangers
    await room.joined;
    room.admit(`daemon-${A_HOST}-${ANOTHER_SESSION}`);

    // Then the connection still reports connected. The handshake overlay is driven off this status
    // and covers the whole pane with `pointer-events-auto`: waiting on a roster could leave a
    // working terminal under a sheet with no way to dismiss it, whereas an absent peer merely makes
    // a call fail — visibly, and recoverably
    expect(session.status).toEqual("connected");
  });

  it("advertises media and presence alongside rpc", () => {
    // Given a session published into a room of its own
    const session = aHostOn(aRecordingTransportFactory().factory, {
      tokens: aTokenMint(),
      newRoom: () => aJoinableRoom().asRoom,
    }).openSession(A_SESSION, attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY));

    // Then the VNC, screen-sharing and roster surfaces apply — what node 4 gates on
    expect([...session.capabilities].sort()).toEqual(["media", "presence", "rpc"]);
  });
});

describe("when a session's token is due for renewal", () => {
  it("is a full minute before it lapses", () => {
    // Then an hour-long token is replaced at 59 minutes — a renewal that landed after expiry would
    // be a reconnect, not a renewal
    expect(tokenRefreshDelayMs(3600n, DEFAULT_TOKEN_REFRESH_POLICY)).toEqual(59 * 60 * 1000);
  });

  it("is never sooner than the floor, however short the token's life", () => {
    // Then a token that lapses inside the lead is renewed on the floor rather than at once. Every
    // renewal carries the same TTL, so a delay of zero re-arms at zero: an unbounded loop at
    // `setTimeout`'s 4ms clamp, aimed at the daemon's own `TokenService`
    expect(tokenRefreshDelayMs(30n, DEFAULT_TOKEN_REFRESH_POLICY)).toEqual(
      DEFAULT_TOKEN_REFRESH_POLICY.minDelayMs,
    );
    expect(tokenRefreshDelayMs(0n, DEFAULT_TOKEN_REFRESH_POLICY)).toEqual(
      DEFAULT_TOKEN_REFRESH_POLICY.minDelayMs,
    );
  });
});

describe("opening a session whose attach reply names no room", () => {
  it("routes over the host connection itself", () => {
    // Given a host that answers its own session RPC — `cli_session_manager.rs` against a PTY handle
    const { factory } = aRecordingTransportFactory();
    const host = aHostOn(factory, { tokens: aTokenMint(), newRoom: () => aJoinableRoom().asRoom });

    // When the session is attached and asks for a client
    const session = host.openSession(
      A_SESSION,
      attachmentHintFromReply(A_SESSION, A_HOST_SERVED_REPLY),
    );

    // Then it is the host's own client, not a second one built over a room that was never named.
    // This is today's `connected-grpc` fallback, except that it is no longer a fallback.
    expect(session.clientFor(ConnectionService)).toBe(host.clientFor(ConnectionService));
  });

  it("advertises rpc only, so the media surfaces do not apply to it", () => {
    // Given a session its host answers itself
    const session = aHostOn(aRecordingTransportFactory().factory, {
      tokens: aTokenMint(),
      newRoom: () => aJoinableRoom().asRoom,
    }).openSession(A_SESSION, attachmentHintFromReply(A_SESSION, A_HOST_SERVED_REPLY));

    // Then nothing a room carries is on offer — the question node 4's gating asks
    expect([...session.capabilities]).toEqual(["rpc"]);
  });

  it("opens without a token client or a room factory at all", () => {
    // Given a provider registered by a build that never joins a session room
    const host = aHostOn(aRecordingTransportFactory().factory, null);

    // When a session the host serves itself is attached over it
    const session = host.openSession(
      A_SESSION,
      attachmentHintFromReply(A_SESSION, A_HOST_SERVED_REPLY),
    );

    // Then the session is ordinary. Needing a media server to attach to a session the host already
    // serves is the coupling this whole node exists to remove.
    expect(session.status).toEqual("connected");
  });
});

describe("a session client's identity", () => {
  it("holds across repeated lookups of the same service", () => {
    // Given one attached session
    const session = aHostOn(aRecordingTransportFactory().factory, {
      tokens: aTokenMint(),
      newRoom: () => aJoinableRoom().asRoom,
    }).openSession(A_SESSION, attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY));

    // When its client is asked for twice — what a host re-rendering does
    const first = session.clientFor(ConnectionService);
    const second = session.clientFor(ConnectionService);

    // Then it is one client. `useAcpReplay` keys an effect on it and cancels an in-flight snapshot
    // pull when it changes, so a re-render that mints a new one loses the Agent Activity feed.
    expect(second).toBe(first);
  });

  it("is addressed somewhere else for a session that is somewhere else", () => {
    // Given two sessions on the same host, each published into a room of its own
    const { factory, routes } = aRecordingTransportFactory();
    const host = aHostOn(factory, {
      tokens: aTokenMint(),
      newRoom: () => aJoinableRoom().asRoom,
    });
    const one = host.openSession(A_SESSION, attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY));
    const other = host.openSession(
      ANOTHER_SESSION,
      attachmentHintFromReply(ANOTHER_SESSION, {
        ...A_ROOM_BACKED_REPLY,
        livekitRoom: "daemon-session-0002",
        livekitServerIdentity: `daemon-${A_HOST}-${ANOTHER_SESSION}`,
      }),
    );

    // When each builds a client
    one.clientFor(ConnectionService);
    other.clientFor(ConnectionService);

    // Then the two are routed to different participants. Asserting merely that the clients are
    // different objects proves nothing here — `openSession` is not memoised, so *any* two calls
    // yield different objects whatever the routing
    expect(routes.map((r) => r.targetIdentity)).toEqual([
      `daemon-${A_HOST}-${A_SESSION}`,
      `daemon-${A_HOST}-${ANOTHER_SESSION}`,
    ]);
  });
});

describe("detaching a session", () => {
  it("releases the room it joined", async () => {
    // Given an attached session holding a joined room
    const room = aJoinableRoom();
    const session = aHostOn(aRecordingTransportFactory().factory, {
      tokens: aTokenMint(),
      newRoom: () => room.asRoom,
    }).openSession(A_SESSION, attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY));
    await room.joined;

    // When it is detached
    session.close();
    await settled();

    // Then the room is released, not merely forgotten. Its lifetime used to be a `useEffect`
    // cleanup, so every session switch leaked a room whenever the hook outlived the attachment.
    expect(room.releaseCount()).toEqual(1);
  });

  it("releases the room once however often it is detached", async () => {
    // Given an attached session holding a joined room
    const room = aJoinableRoom();
    const session = aHostOn(aRecordingTransportFactory().factory, {
      tokens: aTokenMint(),
      newRoom: () => room.asRoom,
    }).openSession(A_SESSION, attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY));
    await room.joined;

    // When the same session is detached twice — an unmount racing an explicit detach
    session.close();
    session.close();
    await settled();

    // Then the room saw exactly one `disconnect()`. A second one races the teardown of a room that
    // is already on its way down; the join's own continuation used to issue it, one microtask after
    // the close, which is precisely where an assertion made in the same tick could not see it
    expect(room.releaseCount()).toEqual(1);
  });

  it("releases the room exactly once when it is detached mid-handshake", async () => {
    // Given a session detached between its token mint and its room connect — the drawer navigating
    // away while the attach is still in flight
    const room = aJoinableRoom();
    const session = aHostOn(aRecordingTransportFactory().factory, {
      tokens: aTokenMint(),
      newRoom: () => room.asRoom,
    }).openSession(A_SESSION, attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY));

    // When it is closed before there is anything joined to disconnect
    session.close();
    await settled();

    // Then the join finished the release the close could not. A `disconnect()` issued before the
    // connect lands does nothing at all, and the connect arriving a moment later would otherwise
    // leave a joined room with nobody holding a reference to close it
    expect(room.releaseCount()).toEqual(1);
  });

  it("refuses to hand out a client afterwards", () => {
    // Given a detached session
    const session = aHostOn(aRecordingTransportFactory().factory, {
      tokens: aTokenMint(),
      newRoom: () => aJoinableRoom().asRoom,
    }).openSession(A_SESSION, attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY));
    session.close();

    // Then a call issued against it says so, rather than being sent somewhere that will never answer
    expect(() => session.clientFor(ConnectionService)).toThrow(
      `session ${A_SESSION} on host ${A_HOST} is closed`,
    );
  });

  it("refuses to hand out a transport afterwards", () => {
    // Given a detached session
    const session = aHostOn(aRecordingTransportFactory().factory, {
      tokens: aTokenMint(),
      newRoom: () => aJoinableRoom().asRoom,
    }).openSession(A_SESSION, attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY));
    session.close();

    // Then building a wire over it says so, rather than handing back one that routes nowhere
    expect(() => session.transport()).toThrow(
      `session ${A_SESSION} on host ${A_HOST} is closed`,
    );
  });

  it("stops offering its room for measurement", async () => {
    // Given an attached session whose room the traffic strip reads round-trip time from
    const room = aJoinableRoom();
    const session = aHostOn(aRecordingTransportFactory().factory, {
      tokens: aTokenMint(),
      newRoom: () => room.asRoom,
    }).openSession(A_SESSION, attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY));
    await room.joined;

    // When it is detached
    session.close();

    // Then there is no room to measure. `StatusBar` reads this straight off the attachment, which
    // can outlive the connection it names by a render — and a released room's peer connection is
    // going away, so a ping taken on it measures a wire nobody is on
    expect(liveKitRoomOf(session)).toBeNull();
  });
});

