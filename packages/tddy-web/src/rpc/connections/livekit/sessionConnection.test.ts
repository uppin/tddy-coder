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
 * Changeset: `docs/dev/1-WIP/2026-09-05-optional-livekit-session-connection.md`
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

/** What it replies for a session it serves itself — the shape a desktop app over IPC produces. */
const A_HOST_SERVED_REPLY = { livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" };

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

/** A mint whose `refreshed` promise settles when the TTL timer comes back for a second token. */
function aTokenMintAwaitingRefresh(ttlSeconds: bigint) {
  let announceRefresh: (identity: string) => void = () => {};
  const refreshed = new Promise<string>((resolve) => {
    announceRefresh = resolve;
  });
  const tokens = {
    generateToken: async ({ identity }: { room: string; identity: string }) => ({
      token: `token-for-${identity}`,
      ttlSeconds,
    }),
    refreshToken: async ({ identity }: { room: string; identity: string }) => {
      announceRefresh(identity);
      return { token: `refreshed-token-for-${identity}`, ttlSeconds };
    },
  } as unknown as Client<typeof TokenService>;
  return { tokens, refreshed };
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
  it("addresses the participant the session's own process serves on", () => {
    // Given a host whose session is published into a room of its own
    const { factory, routes } = aRecordingTransportFactory();
    const session = aHostOn(factory, {
      tokens: aTokenMint(),
      newRoom: () => aJoinableRoom().asRoom,
    }).openSession(A_SESSION, attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY));

    // When it builds a client
    session.clientFor(ConnectionService);

    // Then the calls go to the session process, not to the daemon participant — which is where a
    // session RPC quietly landed whenever the LiveKit path was not taken
    expect(routes.map((r) => r.targetIdentity)).toEqual([`daemon-${A_HOST}-${A_SESSION}`]);
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

  it("re-mints the token before it lapses, so a long session is not dropped at expiry", async () => {
    // Given a token that expires the moment it is issued — the refresh is due immediately
    const mint = aTokenMintAwaitingRefresh(0n);
    const { factory } = aRecordingTransportFactory();
    aHostOn(factory, { tokens: mint.tokens, newRoom: () => aJoinableRoom().asRoom }).openSession(
      A_SESSION,
      attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY),
    );

    // When the refresh timer comes round
    const refreshedFor = await mint.refreshed;

    // Then it renews the same observer's token rather than joining as somebody new
    expect(refreshedFor).toMatch(/^web-traffic-[a-z0-9]+$/);
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

  it("stays connecting until the session's process is on the room", async () => {
    // Given a joined room the session process has not reached yet
    const room = aJoinableRoom();
    const { factory } = aRecordingTransportFactory();
    const session = aHostOn(factory, {
      tokens: aTokenMint(),
      newRoom: () => room.asRoom,
    }).openSession(A_SESSION, attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY));
    await room.joined;

    // Then a call has nowhere to land, which is what `connecting` says
    expect(session.status).toEqual("connecting");

    // When the session's own process joins
    room.admit(`daemon-${A_HOST}-${A_SESSION}`);

    // Then the connection is usable, and the handshake overlay can come down
    expect(session.status).toEqual("connected");
  });

  it("advertises media and presence alongside rpc", () => {
    const session = aHostOn(aRecordingTransportFactory().factory, {
      tokens: aTokenMint(),
      newRoom: () => aJoinableRoom().asRoom,
    }).openSession(A_SESSION, attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY));

    // Then the VNC, screen-sharing and roster surfaces apply — what node 4 gates on
    expect([...session.capabilities].sort()).toEqual(["media", "presence", "rpc"]);
  });
});

describe("opening a session whose attach reply names no room", () => {
  it("routes over the host connection itself", () => {
    // Given a host that answers its own session RPC — `cli_session_manager.rs` against a PTY handle
    const { factory } = aRecordingTransportFactory();
    const host = aHostOn(factory, { tokens: aTokenMint(), newRoom: () => aJoinableRoom().asRoom });
    const session = host.openSession(
      A_SESSION,
      attachmentHintFromReply(A_SESSION, A_HOST_SERVED_REPLY),
    );

    // Then it is the host's own client, not a second one built over a room that was never named.
    // This is today's `connected-grpc` fallback, except that it is no longer a fallback.
    expect(session.clientFor(ConnectionService)).toBe(host.clientFor(ConnectionService));
  });

  it("advertises rpc only, so the media surfaces do not apply to it", () => {
    const session = aHostOn(aRecordingTransportFactory().factory, {
      tokens: aTokenMint(),
      newRoom: () => aJoinableRoom().asRoom,
    }).openSession(A_SESSION, attachmentHintFromReply(A_SESSION, A_HOST_SERVED_REPLY));

    expect([...session.capabilities]).toEqual(["rpc"]);
  });

  it("opens without a token client or a room factory at all", () => {
    // Given a provider registered by a build that never joins a session room
    const session = aHostOn(aRecordingTransportFactory().factory, null).openSession(
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

  it("is fresh for a session routed somewhere else", () => {
    // Given two sessions on the same host
    const host = aHostOn(aRecordingTransportFactory().factory, {
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

    // Then each has its own client, because each genuinely routes somewhere else — a real routing
    // change still has to be picked up
    expect(other.clientFor(ConnectionService)).not.toBe(one.clientFor(ConnectionService));
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

    // Then the room is released, not merely forgotten. Its lifetime used to be a `useEffect`
    // cleanup, so every session switch leaked a room whenever the hook outlived the attachment.
    expect(room.releaseCount()).toEqual(1);
  });

  it("releases the room once however often it is detached", async () => {
    const room = aJoinableRoom();
    const session = aHostOn(aRecordingTransportFactory().factory, {
      tokens: aTokenMint(),
      newRoom: () => room.asRoom,
    }).openSession(A_SESSION, attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY));
    await room.joined;

    // When the same session is detached twice — an unmount racing an explicit detach
    session.close();
    session.close();

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
    const session = aHostOn(aRecordingTransportFactory().factory, {
      tokens: aTokenMint(),
      newRoom: () => aJoinableRoom().asRoom,
    }).openSession(A_SESSION, attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY));
    session.close();

    expect(() => session.transport()).toThrow(
      `session ${A_SESSION} on host ${A_HOST} is closed`,
    );
  });
});

