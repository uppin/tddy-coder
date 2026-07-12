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
import { Room } from "livekit-client";
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
