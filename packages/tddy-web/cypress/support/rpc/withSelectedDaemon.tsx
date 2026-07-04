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
): React.ReactElement {
  return (
    <AuthProvider>
      <SelectedDaemonProvider room={new Room()} daemons={daemons}>
        {children}
      </SelectedDaemonProvider>
    </AuthProvider>
  );
}
