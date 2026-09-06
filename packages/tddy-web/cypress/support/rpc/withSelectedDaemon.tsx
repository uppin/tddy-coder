/**
 * Shared test-only wrapper providing the minimal `SelectedDaemonProvider` fixture a mounted
 * screen needs for `useDaemonClient(ConnectionService)` (and other daemon-level RPC hooks) to
 * resolve to a non-null client.
 *
 * `SessionsDrawerScreen` (and anything it renders) now sources `ConnectionService` via
 * `useDaemonClient`, which returns `null` without a `SelectedDaemonProvider` ancestor providing a
 * connected `room` and at least one daemon. `mountWithRpc` / `mountWithRecordingLiveKitRpc`
 * (`./inMemory.tsx`, `./recordingLiveKitRpc.tsx`) already route *both* the HTTP and LiveKit
 * transports to the same in-memory backend regardless of the LiveKit room/target identity — so a
 * single always-selected fixture daemon plus a fresh, never-connected `Room` instance is all
 * `useDaemonClient` needs to build a client over that same backend.
 *
 * PRD: docs/ft/web/daemon-selector-livekit-rpc.md.
 */

import React from "react";
import { ConnectionState, Room } from "livekit-client";
import type { DaemonHost } from "../../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../../src/rpc/selectedDaemon";
import { AuthProvider } from "../../../src/hooks/authProvider";

/** Default single-daemon fixture — matches the "one local daemon" shape used across other tests. */
export const DEFAULT_TEST_DAEMON: DaemonHost = { instanceId: "local", label: "local (this daemon)" };

/**
 * A minimal stand-in for a connected common-room `Room` carrying a fixed set of remote participant
 * identities — enough for `useRoomParticipants` (which reads `remoteParticipants`, `localParticipant`
 * and the join/leave event hooks). Cross-host tests seed the coder identities
 * (`daemon-<instanceId>-<sessionId>`) that make a session count as having a live participant.
 */
export function aFakeCommonRoom(participantIdentities: string[]): Room {
  const remoteParticipants = new Map(
    participantIdentities.map((identity) => [identity, { identity, metadata: "", joinedAt: new Date() }]),
  );
  return {
    localParticipant: undefined,
    remoteParticipants,
    state: ConnectionState.Connected,
    on: () => {},
    off: () => {},
  } as unknown as Room;
}

/**
 * A `Room` double standing in for a common room this page has **joined**.
 *
 * `new Room()` is not that, and the difference is visible to anything reading the published room
 * status: a freshly constructed SDK room reports `ConnectionState.Disconnected`, which
 * `useCommonRoom` maps to `"connecting"`. A fixture built on one is therefore saying "still
 * joining" — the state a real page is in while its token is being minted — not "joined". Tests that
 * mean "connected, and here is the fleet" want this instead.
 */
export function aJoinedCommonRoom(participantIdentities: string[] = []): Room {
  return aFakeCommonRoom(participantIdentities);
}

/**
 * Like `aFakeCommonRoom`, but each participant carries its own `metadata` string (e.g. a JSON
 * document with a `session` block). Used by acceptance tests that assert presence-driven
 * session metadata rendering in the sessions drawer.
 */
export function aFakeCommonRoomWithMetadata(
  participants: ReadonlyArray<{ identity: string; metadata: string }>,
): Room {
  const remoteParticipants = new Map(
    participants.map((p) => [p.identity, { identity: p.identity, metadata: p.metadata, joinedAt: new Date() }]),
  );
  return {
    localParticipant: undefined,
    remoteParticipants,
    state: ConnectionState.Connected,
    on: () => {},
    off: () => {},
  } as unknown as Room;
}

/**
 * Wrap `children` in a `SelectedDaemonProvider` pre-seeded with `daemons` (default: a single
 * fixture daemon) and a fresh `Room` — enough for `useDaemonClient` to resolve non-null. Also
 * provides `AuthProvider`, since every real daemon-mode screen (`SessionsDrawerScreen`,
 * `WorktreesAppPage`, etc.) reads its session token via `useAuthContext()`. Callers that already
 * wrap their tree in an explicit `AuthProvider` (e.g. to assert on its own refresh behavior) simply
 * get a redundant, harmless nested provider — the nearest one wins for context reads.
 */
export function withSelectedDaemon(
  children: React.ReactNode,
  daemons: DaemonHost[] = [DEFAULT_TEST_DAEMON],
  participantIdentities?: string[],
): React.ReactElement {
  const room =
    participantIdentities !== undefined ? aFakeCommonRoom(participantIdentities) : new Room();
  return (
    <AuthProvider>
      <SelectedDaemonProvider room={room} daemons={daemons}>
        {children}
      </SelectedDaemonProvider>
    </AuthProvider>
  );
}

/**
 * Variant of {@link withSelectedDaemon} that also names the daemon that served the web bundle
 * (`/api/config`'s `daemon_instance_id`) — the one host the browser's own HTTP transport reaches,
 * and therefore the only host a surface holding an HTTP client can read without going through the
 * common room.
 *
 * Pass it first in `daemons` too, as a real deployment does: the selection resolves to the serving
 * daemon anyway, so a cross-host test states one "this is the host we are connected to" rather than
 * leaving the two ideas free to disagree over something the test is not about.
 */
export function withSelectedDaemonServedBy(
  children: React.ReactNode,
  daemons: DaemonHost[],
  servingInstanceId: string,
): React.ReactElement {
  return (
    <AuthProvider>
      <SelectedDaemonProvider
        room={new Room()}
        daemons={daemons}
        servingInstanceId={servingInstanceId}
      >
        {children}
      </SelectedDaemonProvider>
    </AuthProvider>
  );
}

/**
 * Variant of {@link withSelectedDaemon} that takes a pre-built common-room `Room` (e.g. one from
 * `aFakeCommonRoomWithMetadata`) so a test can seed participant metadata for presence-driven
 * rendering assertions.
 */
export function withSelectedDaemonRoom(
  children: React.ReactNode,
  daemons: DaemonHost[],
  room: Room,
): React.ReactElement {
  return (
    <AuthProvider>
      <SelectedDaemonProvider room={room} daemons={daemons}>
        {children}
      </SelectedDaemonProvider>
    </AuthProvider>
  );
}
