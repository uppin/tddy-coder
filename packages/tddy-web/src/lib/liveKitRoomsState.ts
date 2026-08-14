/**
 * The rooms panel's state, as pure functions over the `StreamLiveKitRooms` feed.
 *
 * The feed's contract is snapshot-then-changes: its first message carries every room, and every
 * message after it carries exactly one delta. Keeping the fold here — rather than inside
 * `useLiveKitRooms` — is what makes "a `participant_left` decrements the count" a unit test instead
 * of a mounted-component test.
 *
 * PRD: `docs/ft/web/livekit-rooms-panel.md`
 */

import type {
  LiveKitParticipantInfo,
  LiveKitRoomInfo,
  LiveKitRoomsChange,
} from "../gen/connection_pb";
import { inferParticipantRole, type ParticipantRole } from "./participantRole";

export interface LiveKitRoomParticipant {
  identity: string;
  name: string;
  metadata: string;
  joinedAtMs: number;
  /** The server's participant state: `JOINED`, `ACTIVE` or `DISCONNECTED`. */
  state: string;
  /** Derived with the same grammar the connected-participants panel uses. */
  role: ParticipantRole;
}

export interface LiveKitRoom {
  name: string;
  /** A human name from the room metadata's `label`, or `null` when it carries none. */
  label: string | null;
  createdAtMs: number;
  metadata: string;
  /** Sorted by identity. The room's participant count is this array's length. */
  participants: LiveKitRoomParticipant[];
}

/**
 * A `label` string from a room's metadata JSON, or `null` when the metadata is absent, unparseable,
 * carries no `label`, or carries a blank one. Nothing publishes room metadata today, so `null` is
 * the normal answer.
 */
export function roomLabelFromMetadata(metadata: string): string | null {
  const trimmed = metadata.trim();
  if (!trimmed.startsWith("{")) return null;
  try {
    const parsed = JSON.parse(trimmed) as { label?: unknown };
    if (typeof parsed.label !== "string") return null;
    const label = parsed.label.trim();
    return label ? label : null;
  } catch {
    return null;
  }
}

/** The whole state, from the feed's first message. Rooms sorted by name, participants by identity. */
export function roomsFromSnapshot(rooms: LiveKitRoomInfo[]): LiveKitRoom[] {
  return rooms.map(roomFromInfo).sort(byName);
}

/**
 * Fold one change event onto the current rooms, returning the new state.
 *
 * A change naming a room that is not in `rooms` is ignored — only `room_added` carries an
 * authoritative row, so inventing a room from a partial event would render facts the server never
 * sent.
 */
export function applyRoomsChange(
  rooms: LiveKitRoom[],
  change: LiveKitRoomsChange,
): LiveKitRoom[] {
  switch (change.change.case) {
    case "roomAdded": {
      const added = change.change.value.room;
      if (!added) return rooms;
      return [...rooms.filter((r) => r.name !== added.name), roomFromInfo(added)].sort(byName);
    }
    case "roomRemoved": {
      const removed = change.change.value.room;
      return rooms.filter((r) => r.name !== removed);
    }
    case "participantJoined": {
      const { room, participant } = change.change.value;
      if (!participant) return rooms;
      return withRoom(rooms, room, (known) => ({
        ...known,
        participants: [
          ...known.participants.filter((p) => p.identity !== participant.identity),
          participantFromInfo(participant),
        ].sort(byIdentity),
      }));
    }
    case "participantLeft": {
      const { room, identity } = change.change.value;
      return withRoom(rooms, room, (known) => ({
        ...known,
        participants: known.participants.filter((p) => p.identity !== identity),
      }));
    }
    case "participantMetadataChanged": {
      const { room, identity, metadata } = change.change.value;
      return withRoom(rooms, room, (known) => ({
        ...known,
        participants: known.participants.map((p) =>
          p.identity === identity
            ? { ...p, metadata, role: inferParticipantRole(p.identity, metadata) }
            : p,
        ),
      }));
    }
    case "participantStateChanged": {
      const { room, identity, state } = change.change.value;
      return withRoom(rooms, room, (known) => ({
        ...known,
        participants: known.participants.map((p) =>
          p.identity === identity ? { ...p, state } : p,
        ),
      }));
    }
    case undefined:
      return rooms;
  }
}

/**
 * Replace the named room with `update`'s result, or return `rooms` unchanged when the change names a
 * room the client has never been told about.
 */
function withRoom(
  rooms: LiveKitRoom[],
  name: string,
  update: (known: LiveKitRoom) => LiveKitRoom,
): LiveKitRoom[] {
  if (!rooms.some((r) => r.name === name)) return rooms;
  return rooms.map((r) => (r.name === name ? update(r) : r));
}

function roomFromInfo(info: LiveKitRoomInfo): LiveKitRoom {
  return {
    name: info.name,
    label: roomLabelFromMetadata(info.metadata),
    createdAtMs: Number(info.createdAtMs),
    metadata: info.metadata,
    participants: info.participants.map(participantFromInfo).sort(byIdentity),
  };
}

function participantFromInfo(info: LiveKitParticipantInfo): LiveKitRoomParticipant {
  return {
    identity: info.identity,
    name: info.name,
    metadata: info.metadata,
    joinedAtMs: Number(info.joinedAtMs),
    state: info.state,
    role: inferParticipantRole(info.identity, info.metadata),
  };
}

const byName = (a: LiveKitRoom, b: LiveKitRoom): number =>
  a.name < b.name ? -1 : a.name > b.name ? 1 : 0;

const byIdentity = (a: LiveKitRoomParticipant, b: LiveKitRoomParticipant): number =>
  a.identity < b.identity ? -1 : a.identity > b.identity ? 1 : 0;
