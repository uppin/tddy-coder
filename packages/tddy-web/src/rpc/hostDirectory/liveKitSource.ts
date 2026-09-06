/**
 * The common room as a host-directory source.
 *
 * This is the whole of what `SelectedDaemonProvider` used to do inline: join the common room, read
 * its participants, and keep the ones whose metadata parses as a daemon advertisement. It produces
 * exactly the list it produced before — `daemonHostsFromParticipants` is unchanged and still the
 * only place a participant becomes a host — but now says so as one contributor among several, so a
 * page with no common room simply has one fewer.
 *
 * Everything LiveKit-shaped about the directory lives here. A build that never joins a room never
 * calls this hook, and nothing else in `hostDirectory/` imports `livekit-client`.
 */

import { useMemo } from "react";
import type { Room } from "livekit-client";
import { useCommonRoom, useObservedCommonRoomStatus } from "../../hooks/useCommonRoom";
import { useRoomParticipants } from "../../hooks/useRoomParticipants";
import { daemonHostsFromParticipants, type DaemonHost } from "../../lib/participantRole";
import { hostDescriptorOf } from "./daemonHost";
import type { HostDirectorySource } from "./types";

/** The id this source contributes under. Precedence is stated against it, so it is a constant. */
export const LIVEKIT_SOURCE_ID = "livekit";

export interface LiveKitHostDirectorySourceOptions {
  livekitUrl?: string;
  commonRoom?: string;
  /** This browser's presence identity, or `undefined` until the operator is signed in. */
  identity?: string;
  /**
   * Test-injection seam: an already-joined `Room`, used instead of joining one. Distinct from
   * `undefined`, which means "no override" — `null` is a test asserting there is no room.
   */
  room?: Room | null;
  /** Test-injection seam: the host list, used instead of deriving one from the room's roster. */
  hosts?: DaemonHost[];
  /**
   * Test-injection seam: the `Room` object the join is performed with. Unlike the two above, this
   * leaves the source on its production path, so a test can drive a join that fails or never
   * settles.
   */
  roomFactory?: () => Room;
}

/**
 * The common room's contribution to the directory, and the room itself.
 *
 * The room comes back because two things above the directory still need the object: the connection
 * provider that reaches hosts over it, and the presence context a media surface reads. Neither is
 * the directory's business, which is why this hook hands the room over rather than publishing it.
 *
 * **With no `livekitUrl`/`commonRoom` this constructs no `Room` and mints no token** — `useCommonRoom`
 * short-circuits on its own guard before either — and the source reports `idle` with no hosts. That
 * is the distinction the whole of "LiveKit is optional" rests on: an unconfigured common room is a
 * choice, and reporting it as `error` would put a connection failure on every desktop screen for a
 * feature nobody asked for.
 */
export function useLiveKitHostDirectorySource(options: LiveKitHostDirectorySourceOptions): {
  source: HostDirectorySource;
  room: Room | null;
} {
  const { livekitUrl, commonRoom, identity, room: roomOverride, hosts: hostsOverride } = options;
  const {
    room: joinedRoom,
    status: joinStatus,
    error: joinError,
  } = useCommonRoom(livekitUrl, commonRoom, identity, options.roomFactory);
  const room = roomOverride !== undefined ? roomOverride : joinedRoom;

  const participants = useRoomParticipants(hostsOverride !== undefined ? null : room);
  const derivedHosts = useMemo(() => daemonHostsFromParticipants(participants), [participants]);
  const daemonHosts = hostsOverride !== undefined ? hostsOverride : derivedHosts;
  const hosts = useMemo(
    () => daemonHosts.map((host) => hostDescriptorOf(host, LIVEKIT_SOURCE_ID)),
    [daemonHosts],
  );

  // The status rule is the one the provider published before, verbatim: until there is a room
  // object it is the outcome of the join attempt, and once there is one it is that room's own live
  // connection state, so a drop after a successful join is reported too. It also covers the `room`
  // override without special-casing it — an injected room speaks for itself.
  const observed = useObservedCommonRoomStatus(room);
  const status = room ? observed.status : joinStatus;
  const error = room ? observed.error : joinError;

  const source = useMemo<HostDirectorySource>(
    () => ({ id: LIVEKIT_SOURCE_ID, status, error, hosts }),
    [status, error, hosts],
  );
  return { source, room };
}
