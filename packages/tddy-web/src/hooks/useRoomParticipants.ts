import { useEffect, useState } from "react";
import { Room, RoomEvent, type Participant } from "livekit-client";

import {
  parseCodexOAuthMetadata,
  type CodexOAuthInfo,
} from "../lib/codexOauthMetadata";
import {
  inferParticipantRole,
  metadataLooksLikeDaemonAdvertisement,
  type ParticipantRole,
} from "../lib/participantRole";

// Re-exported from the canonical `lib/participantRole` module (kept here for existing importers).
export {
  inferParticipantRole,
  metadataLooksLikeDaemonAdvertisement,
  type ParticipantRole,
};

/** Must match `OWNED_PROJECT_COUNT_METADATA_KEY` in `tddy-livekit` (`participant.rs`). */
export const OWNED_PROJECT_COUNT_METADATA_KEY = "owned_project_count";

/** Non-negative integer from LiveKit participant metadata JSON, when present. */
export function parseOwnedProjectCount(metadata: string): number | null {
  const t = metadata.trim();
  if (!t.startsWith("{")) return null;
  try {
    const o = JSON.parse(t) as Record<string, unknown>;
    const n = o[OWNED_PROJECT_COUNT_METADATA_KEY];
    if (typeof n === "number" && Number.isInteger(n) && n >= 0) return n;
    return null;
  } catch {
    return null;
  }
}

export interface RoomParticipant {
  identity: string;
  role: ParticipantRole;
  joinedAt: number | null;
  metadata: string;
  /** Structured Codex OAuth hint from metadata JSON, when present. */
  codexOAuth: CodexOAuthInfo | null;
  /** From `owned_project_count` in metadata; omit in tests to parse from `metadata` in the UI. */
  ownedProjectCount?: number | null;
}

function mapParticipant(p: Participant): RoomParticipant {
  const metadata = p.metadata ?? "";
  const ownedProjectCount = parseOwnedProjectCount(metadata);
  if (ownedProjectCount !== null) {
    console.debug("[tddy-web:presence] participant owned_project_count", {
      identity: p.identity,
      ownedProjectCount,
    });
  }
  return {
    identity: p.identity,
    role: inferParticipantRole(p.identity, metadata),
    joinedAt: p.joinedAt?.getTime() ?? null,
    metadata,
    codexOAuth: parseCodexOAuthMetadata(metadata),
    ownedProjectCount,
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
