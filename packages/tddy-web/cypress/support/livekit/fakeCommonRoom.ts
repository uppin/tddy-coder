/**
 * Test double for the common-room LiveKit `Room` that the daemon selector's live data path
 * consumes (`useCommonRoom` -> `useRoomParticipants` -> `daemonHostsFromParticipants` ->
 * `SelectedDaemonProvider`).
 *
 * The production provider derives its daemon list *purely* from LiveKit room events, so faithfully
 * reproducing "the selector didn't update / went empty" requires a room that actually emits the
 * real `RoomEvent`s. This is deliberately NOT the static `daemons` injection seam used by the other
 * acceptance tests: that seam feeds `useRoomParticipants` a `null` room and bypasses the live event
 * wiring entirely, so it cannot catch a missing event subscription.
 */

import { RoomEvent, type Room } from "livekit-client";
import type { DaemonHost } from "../../../src/lib/participantRole";

/**
 * A daemon common-room advertisement, matching the JSON shape parsed by `parseDaemonAdvertisement`.
 * Kept inline rather than importing `src/test-utils` (that module is bun:test-only — see its header).
 */
function daemonAdvertisement(host: DaemonHost): string {
  return JSON.stringify({ instance_id: host.instanceId, label: host.label });
}

type Listener = (...args: unknown[]) => void;

/** A single common-room participant, shaped for what `useRoomParticipants`'s `buildList` reads. */
interface FakeParticipant {
  identity: string;
  metadata: string;
  joinedAt?: Date;
}

export interface FakeCommonRoom {
  /** The object to pass as `SelectedDaemonProvider`'s `room` prop. */
  readonly room: Room;
  /** Pre-seed daemons present at mount, so the initial sync already sees them. Chainable. */
  withDaemons(hosts: DaemonHost[]): FakeCommonRoom;
  /** A new daemon joins after the initial connection — fires `ParticipantConnected`. */
  connectDaemon(host: DaemonHost): void;
  /** A daemon leaves the common room — fires `ParticipantDisconnected`. */
  disconnectDaemon(instanceId: string): void;
  /**
   * A LiveKit auto-reconnect re-establishes the connection and re-syncs the participant roster to
   * `hosts`, announced via a single `RoomEvent.Reconnected` (not per-peer `ParticipantConnected`) —
   * exactly how existing peers come back after a network blip, LiveKit restart, or dev-server churn.
   */
  reconnectWith(hosts: DaemonHost[]): void;
}

export function aFakeCommonRoom(): FakeCommonRoom {
  const listeners = new Map<string, Set<Listener>>();
  const remoteParticipants = new Map<string, FakeParticipant>();

  const emit = (event: string) => {
    listeners.get(event)?.forEach((cb) => cb());
  };

  const setRoster = (hosts: DaemonHost[]) => {
    remoteParticipants.clear();
    for (const host of hosts) {
      remoteParticipants.set(host.instanceId, {
        identity: host.instanceId,
        metadata: daemonAdvertisement(host),
      });
    }
  };

  const room = {
    localParticipant: null,
    remoteParticipants,
    on(event: string, cb: Listener) {
      const set = listeners.get(event) ?? new Set<Listener>();
      set.add(cb);
      listeners.set(event, set);
      return room;
    },
    off(event: string, cb: Listener) {
      listeners.get(event)?.delete(cb);
      return room;
    },
  } as unknown as Room;

  const controller: FakeCommonRoom = {
    room,
    withDaemons(hosts) {
      setRoster(hosts);
      return controller;
    },
    connectDaemon(host) {
      remoteParticipants.set(host.instanceId, {
        identity: host.instanceId,
        metadata: daemonAdvertisement(host),
      });
      emit(RoomEvent.ParticipantConnected);
    },
    disconnectDaemon(instanceId) {
      remoteParticipants.delete(instanceId);
      emit(RoomEvent.ParticipantDisconnected);
    },
    reconnectWith(hosts) {
      setRoster(hosts);
      emit(RoomEvent.Reconnected);
    },
  };

  return controller;
}
