/**
 * Unit tests for the LiveKit connection provider.
 *
 * This is the only provider that exists today, so everything the rest of the app believes about
 * "reaching a host" is really a belief about this class. Four of those beliefs are load-bearing:
 *
 *   • **It claims nothing without a room.** The entire "LiveKit is optional" claim rests on this
 *     single fact — a build that never joins a common room registers this provider with a `null`
 *     room, every host resolves to `null`, and each screen renders its ordinary "not connected"
 *     state instead of failing. If this one regressed, "no LiveKit" would look like a crash.
 *   • **An absent peer is `connecting`, not an error.** Machines come and go from a fleet.
 *   • **A host resolves to the same connection object every time**, because `clientFor` memoises per
 *     connection and a consumer keying an effect on the client depends on that identity holding.
 *   • **It advertises `{rpc, media, presence}`** — the capability set node 4 gates media and
 *     presence surfaces on, and the reason a capability is a property of the wire rather than of the
 *     machine.
 *
 * No React and no real `Room` are needed for any of it: `LiveKitHostConnection` reads exactly two
 * things off the room, so a literal with those two things is a faithful stand-in and a far more
 * legible one than a half-connected SDK object.
 *
 * Changeset: `docs/dev/1-WIP/2026-09-05-optional-livekit-connection-model.md`
 */

import { describe, it, expect } from "bun:test";
import type { Transport } from "@connectrpc/connect";
import { ConnectionState, type Room } from "livekit-client";
import { daemonRpcIdentity } from "../../lib/participantRole";
import { LiveKitConnectionProvider, LIVEKIT_PROVIDER_ID } from "./liveKit";

const A_HOST = "workstation-1";
const ANOTHER_HOST = "laptop-b";

/**
 * A room holding exactly what a connection reads off one: its connection state and who is on it.
 *
 * `remoteParticipants` is keyed by participant *identity*, and a daemon serves its RPC under
 * `daemon-{instanceId}` rather than under its bare instance id — see `lib/participantRole`. Naming
 * the hosts here and deriving the keys is what keeps this fixture honest about that.
 */
function aRoomWith(
  state: ConnectionState,
  hostsOnIt: string[] = [],
): Room {
  const remoteParticipants = new Map(
    hostsOnIt.map((hostId) => [daemonRpcIdentity(hostId), { identity: daemonRpcIdentity(hostId) }]),
  );
  return { state, remoteParticipants } as unknown as Room;
}

/** A transport factory that builds nothing real — these tests assert routing, never traffic. */
function aStubFactory(): () => Transport {
  return () =>
    ({
      unary: () => {
        throw new Error("this transport is a routing stand-in and issues no calls");
      },
      stream: () => {
        throw new Error("this transport is a routing stand-in and issues no calls");
      },
    }) as unknown as Transport;
}

describe("LiveKitConnectionProvider", () => {
  it("claims no host at all while it has no room", () => {
    // Given a provider constructed before the join, or in a build that never joins one — the
    // desktop app's default
    const provider = new LiveKitConnectionProvider(null, aStubFactory());

    // When any host is asked for
    const connection = provider.connectHost(A_HOST);

    // Then it claims nothing. This is the fact the whole "LiveKit is optional" position rests on:
    // with no room there is no participant to name, so the registry falls through to whatever
    // other wire was registered, and with none registered every screen renders "not connected"
    // rather than throwing.
    expect(connection).toBeNull();
  });

  it("claims no host for an empty host id", () => {
    // Given a joined room
    const provider = new LiveKitConnectionProvider(
      aRoomWith(ConnectionState.Connected, [A_HOST]),
      aStubFactory(),
    );

    // When the caller has no host selected yet and passes the empty id through
    const connection = provider.connectHost("");

    // Then that is "nothing selected", not a host named by the empty string
    expect(connection).toBeNull();
  });

  it("reports a host that is not on the room as connecting rather than as an error", () => {
    // Given a joined room that another daemon is on, but not the one being asked about
    const provider = new LiveKitConnectionProvider(
      aRoomWith(ConnectionState.Connected, [ANOTHER_HOST]),
      aStubFactory(),
    );

    // When that host is resolved
    const connection = provider.connectHost(A_HOST);

    // Then it is claimed — the roster is the host *directory*'s business, not this provider's —
    // and reports `connecting`, because a machine that is off right now may be on in a moment.
    // Calling that an error would put a fault banner in front of an ordinary fleet state.
    expect(connection).not.toBeNull();
    expect(connection?.status).toEqual("connecting");
    expect(connection?.error).toBeNull();
  });

  it("reports a host that is on the room as connected", () => {
    // Given a joined room the host serves its RPC on
    const provider = new LiveKitConnectionProvider(
      aRoomWith(ConnectionState.Connected, [A_HOST, ANOTHER_HOST]),
      aStubFactory(),
    );

    // When it is resolved
    const connection = provider.connectHost(A_HOST);

    // Then it is reachable, and says which wire reached it
    expect(connection?.status).toEqual("connected");
    expect(connection?.hostId).toEqual(A_HOST);
    expect(connection?.providerId).toEqual(LIVEKIT_PROVIDER_ID);
  });

  it("reports every host as connecting while the room itself is still joining", () => {
    // Given a room that is up but not yet joined — the window between the token mint and the
    // connect, which every page load passes through
    const provider = new LiveKitConnectionProvider(
      aRoomWith(ConnectionState.Connecting, [A_HOST]),
      aStubFactory(),
    );

    // When a host is resolved
    const connection = provider.connectHost(A_HOST);

    // Then it is not yet reachable, however the roster reads: nothing can be sent over a room
    // that has not finished joining
    expect(connection?.status).toEqual("connecting");
  });

  it("re-reads the roster on every ask rather than capturing it", () => {
    // Given a host that is on the room when it is first resolved
    const room = aRoomWith(ConnectionState.Connected, [A_HOST]);
    const provider = new LiveKitConnectionProvider(room, aStubFactory());
    const connection = provider.connectHost(A_HOST);
    expect(connection?.status).toEqual("connected");

    // When it leaves, with nothing re-resolving the connection
    room.remoteParticipants.delete(daemonRpcIdentity(A_HOST));

    // Then the connection already handed out says so. A caller refusing to send into a stream
    // nobody is reading has to see that at the moment of the send, not at the moment it resolved.
    expect(connection?.status).toEqual("connecting");
  });

  it("hands back the same connection object for the same host", () => {
    // Given a joined room
    const provider = new LiveKitConnectionProvider(
      aRoomWith(ConnectionState.Connected, [A_HOST]),
      aStubFactory(),
    );

    // When one host is resolved twice — four screens on one page do exactly this
    const first = provider.connectHost(A_HOST);
    const second = provider.connectHost(A_HOST);

    // Then it is one connection, which is what makes `clientFor` memoisable: a consumer keying an
    // effect on the client must not have its stream torn down because a second screen asked
    expect(second).toBe(first!);
  });

  it("keeps one connection per host, not one for the whole room", () => {
    // Given two hosts on the room
    const provider = new LiveKitConnectionProvider(
      aRoomWith(ConnectionState.Connected, [A_HOST, ANOTHER_HOST]),
      aStubFactory(),
    );

    // When both are resolved
    const one = provider.connectHost(A_HOST);
    const other = provider.connectHost(ANOTHER_HOST);

    // Then they are distinct, each addressing its own participant
    expect(other).not.toBe(one!);
    expect(one?.hostId).toEqual(A_HOST);
    expect(other?.hostId).toEqual(ANOTHER_HOST);
  });

  it("advertises RPC, media and presence on every host it reaches", () => {
    // Given a host reached over the common room
    const provider = new LiveKitConnectionProvider(
      aRoomWith(ConnectionState.Connected, [A_HOST]),
      aStubFactory(),
    );

    // When its capabilities are read
    const capabilities = provider.connectHost(A_HOST)!.capabilities;

    // Then all three are offered: a LiveKit room carries tracks and a participant roster alongside
    // its data channel, so this is the wire that can do everything. It is the set node 4 gates the
    // media and presence surfaces on, and the reason a capability belongs to how you are connected
    // rather than to the machine you are connected to — the same host is media-capable here and
    // not over an IPC bridge.
    expect([...capabilities].sort()).toEqual(["media", "presence", "rpc"]);
  });
});
