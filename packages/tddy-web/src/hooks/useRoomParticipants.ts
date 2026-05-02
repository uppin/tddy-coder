import { useEffect, useState } from "react";
import { Room, RoomEvent, type Participant } from "livekit-client";

import {
  parseCodexOAuthMetadata,
  type CodexOAuthInfo,
} from "../lib/codexOauthMetadata";

export type ParticipantRole = "browser" | "coder" | "daemon" | "unknown";

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

/**
 * `tddy-daemon` common-room advertisement (`livekit_peer_discovery`):
 * `{"instance_id":"…","label":"… (this daemon)"}`.
 */
export function metadataLooksLikeDaemonAdvertisement(metadata: string): boolean {
  const t = metadata.trim();
  if (!t.startsWith("{")) return false;
  try {
    const o = JSON.parse(t) as { instance_id?: unknown; label?: unknown };
    if (typeof o.instance_id !== "string" || !o.instance_id.trim()) return false;
    if (typeof o.label !== "string") return false;
    return o.label.includes("(this daemon)");
  } catch {
    return false;
  }
}

/**
 * Infer UI role from LiveKit identity and metadata.
 * - **browser**: dashboard presence (`web-…`, `browser-…`).
 * - **coder**: terminal/session tool side (`server`, `server…`, `daemon-{uuid}-…`).
 * - **daemon**: embedded/CLI `tddy-daemon` in common room (metadata advertisement).
 */
export function inferParticipantRole(identity: string, metadata: string): ParticipantRole {
  if (identity.startsWith("web-") || identity.startsWith("browser-")) return "browser";
  if (
    identity === "server" ||
    identity.startsWith("server") ||
    identity.startsWith("daemon-")
  ) {
    return "coder";
  }
  if (metadataLooksLikeDaemonAdvertisement(metadata)) {
    return "daemon";
  }
  return "unknown";
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
