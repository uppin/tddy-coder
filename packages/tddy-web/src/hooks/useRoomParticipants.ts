import { useEffect, useState } from "react";
import { Room, RoomEvent, type Participant } from "livekit-client";

export type ParticipantRole = "server" | "browser" | "unknown";

export interface RoomParticipant {
  identity: string;
  role: ParticipantRole;
  joinedAt: number | null;
  metadata: string;
}

/** Infer UI role from LiveKit identity (browser clients use `web-{github}`). */
export function inferParticipantRole(identity: string): ParticipantRole {
  if (identity.startsWith("web-") || identity.startsWith("browser-")) return "browser";
  if (
    identity === "server" ||
    identity.startsWith("server") ||
    identity.startsWith("daemon-")
  ) {
    return "server";
  }
  return "unknown";
}

function mapParticipant(p: Participant): RoomParticipant {
  return {
    identity: p.identity,
    role: inferParticipantRole(p.identity),
    joinedAt: p.joinedAt?.getTime() ?? null,
    metadata: p.metadata ?? "",
  };
}

function buildList(room: Room): RoomParticipant[] {
  const list: RoomParticipant[] = [];
  if (room.localParticipant) {
    list.push(mapParticipant(room.localParticipant));
  }
  room.remoteParticipants.forEach((p) => {
    list.push(mapParticipant(p));
  });
  return list.sort((a, b) => a.identity.localeCompare(b.identity));
}

/**
 * Tracks all participants in a LiveKit room (local + remote), updating on join/leave/metadata.
 */
export function useRoomParticipants(room: Room | null): RoomParticipant[] {
  const [participants, setParticipants] = useState<RoomParticipant[]>([]);

  useEffect(() => {
    if (!room) {
      setParticipants([]);
      return;
    }

    const sync = () => {
      setParticipants(buildList(room));
    };

    sync();
    room.on(RoomEvent.ParticipantConnected, sync);
    room.on(RoomEvent.ParticipantDisconnected, sync);
    room.on(RoomEvent.ParticipantMetadataChanged, sync);

    return () => {
      room.off(RoomEvent.ParticipantConnected, sync);
      room.off(RoomEvent.ParticipantDisconnected, sync);
      room.off(RoomEvent.ParticipantMetadataChanged, sync);
    };
  }, [room]);

  return participants;
}
