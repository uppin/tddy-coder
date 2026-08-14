/**
 * Test helpers for the **LiveKit rooms feed** (`ConnectionService.StreamLiveKitRooms`) that backs the
 * rooms panel on `#/livekit`.
 *
 * The daemon's contract is snapshot-then-changes: the first message on a stream is always a full
 * `LiveKitRoomsSnapshot`, and every message after it is a single `LiveKitRoomsChange`. This fake
 * models that faithfully — it emits exactly one snapshot, then tails whatever
 * {@link LiveKitRoomsBackend.pushChange} appends, and never returns. A generator that *completed*
 * would read to the client as the daemon dropping the feed, which is a different fact.
 *
 * PRD: `docs/ft/web/livekit-rooms-panel.md`
 */

import { create } from "@bufbuild/protobuf";
import { Code, ConnectError } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  ConnectionService,
  LiveKitParticipantInfoSchema,
  LiveKitRoomInfoSchema,
  LiveKitRoomsChangeSchema,
  LiveKitRoomsEventSchema,
  type LiveKitParticipantInfo,
  type LiveKitRoomInfo,
  type LiveKitRoomsChange,
} from "../../../src/gen/connection_pb";

// ---------------------------------------------------------------------------
// Fixture builders
// ---------------------------------------------------------------------------

/** 2026-08-14T09:00:00Z — a fixed instant, so rendered timestamps are assertable. */
export const DEFAULT_JOINED_AT_MS = 1_786_431_600_000;
/** 2026-08-14T08:30:00Z — half an hour before the default join, as a room must predate its members. */
export const DEFAULT_CREATED_AT_MS = 1_786_429_800_000;

export interface ParticipantOverrides {
  identity: string;
  name?: string;
  metadata?: string;
  joinedAtMs?: number;
  state?: string;
}

/**
 * A participant joined to a room. Defaults are an active participant carrying no metadata, so a bare
 * `aParticipant({ identity })` is a usable row and a spec names only what its scenario is about.
 */
export function aParticipant(overrides: ParticipantOverrides): LiveKitParticipantInfo {
  return create(LiveKitParticipantInfoSchema, {
    identity: overrides.identity,
    name: overrides.name ?? "",
    metadata: overrides.metadata ?? "",
    joinedAtMs: BigInt(overrides.joinedAtMs ?? DEFAULT_JOINED_AT_MS),
    state: overrides.state ?? "ACTIVE",
  });
}

export interface RoomOverrides {
  name: string;
  createdAtMs?: number;
  participants?: LiveKitParticipantInfo[];
  /** The room's own metadata. A `label` string in it names the room for a human. */
  metadata?: string;
}

/**
 * A room on the LiveKit server. Defaults to an empty, unlabelled room created at
 * {@link DEFAULT_CREATED_AT_MS} — unlabelled because no publisher writes a `label` key today (a
 * session room's metadata is a worktree snapshot), so an unlabelled room is the realistic default
 * rather than a degenerate one.
 */
export function aRoom(overrides: RoomOverrides): LiveKitRoomInfo {
  return create(LiveKitRoomInfoSchema, {
    name: overrides.name,
    createdAtMs: BigInt(overrides.createdAtMs ?? DEFAULT_CREATED_AT_MS),
    participants: overrides.participants ?? [],
    metadata: overrides.metadata ?? "",
  });
}

// ---------------------------------------------------------------------------
// Change-event builders
// ---------------------------------------------------------------------------

export function roomAdded(room: LiveKitRoomInfo): LiveKitRoomsChange {
  return create(LiveKitRoomsChangeSchema, { change: { case: "roomAdded", value: { room } } });
}

export function roomRemoved(room: string): LiveKitRoomsChange {
  return create(LiveKitRoomsChangeSchema, { change: { case: "roomRemoved", value: { room } } });
}

export function participantJoined(
  room: string,
  participant: LiveKitParticipantInfo,
): LiveKitRoomsChange {
  return create(LiveKitRoomsChangeSchema, {
    change: { case: "participantJoined", value: { room, participant } },
  });
}

export function participantLeft(room: string, identity: string): LiveKitRoomsChange {
  return create(LiveKitRoomsChangeSchema, {
    change: { case: "participantLeft", value: { room, identity } },
  });
}

export function participantMetadataChanged(
  room: string,
  identity: string,
  metadata: string,
): LiveKitRoomsChange {
  return create(LiveKitRoomsChangeSchema, {
    change: { case: "participantMetadataChanged", value: { room, identity, metadata } },
  });
}

export function participantStateChanged(
  room: string,
  identity: string,
  state: string,
): LiveKitRoomsChange {
  return create(LiveKitRoomsChangeSchema, {
    change: { case: "participantStateChanged", value: { room, identity, state } },
  });
}

// ---------------------------------------------------------------------------
// Backend
// ---------------------------------------------------------------------------

export interface LiveKitRoomsScenario {
  /** The rooms the stream's first message carries. */
  rooms: LiveKitRoomInfo[];
  /** When set, the stream fails with this reason *instead of* emitting a snapshot. */
  failBeforeSnapshot?: string;
  /** When set, the stream emits the snapshot and *then* fails with this reason. */
  failAfterSnapshot?: string;
}

export interface LiveKitRoomsBackend {
  backend: InMemoryRpcBackend;
  /**
   * How many rooms subscriptions the client opened. Streams are not recorded by the testkit (its
   * interceptor skips `req.stream`), so the fake tallies its own — this is how a spec pins that the
   * panel opened exactly one feed rather than one per room.
   */
  roomsStreamCount: () => number;
  /**
   * How many of those subscriptions the client has since cancelled. Each handler holds the call's
   * own `AbortSignal`, which is linked to the one the client passes in — so this counts calls the
   * *client* ended, which is the only way a feed that is silent while idle can ever be released.
   */
  cancelledRoomsStreamCount: () => number;
  /** Append one change event to the live tail of every open rooms stream. */
  pushChange: (change: LiveKitRoomsChange) => void;
}

export function aLiveKitRoomsBackend(scenario: LiveKitRoomsScenario): LiveKitRoomsBackend {
  const callSignals: AbortSignal[] = [];
  const live = aChangeTail();

  const backend = anInMemoryRpcBackend().implement(ConnectionService, {
    async *streamLiveKitRooms(_req, context) {
      callSignals.push(context.signal);
      if (scenario.failBeforeSnapshot !== undefined) {
        throw new ConnectError(scenario.failBeforeSnapshot, Code.Unavailable);
      }
      yield create(LiveKitRoomsEventSchema, {
        event: { case: "snapshot", value: { rooms: scenario.rooms } },
      });
      if (scenario.failAfterSnapshot !== undefined) {
        throw new ConnectError(scenario.failAfterSnapshot, Code.Unavailable);
      }
      yield* live.changes();
    },
  });

  return {
    backend,
    roomsStreamCount: () => callSignals.length,
    cancelledRoomsStreamCount: () => callSignals.filter((signal) => signal.aborted).length,
    pushChange: (change) => live.push(change),
  };
}

/**
 * The shared live tail behind every open rooms stream. Changes are held in one list and each
 * subscriber walks it at its own cursor, so a remount sees the same tail rather than splitting it —
 * and the generator never returns, keeping the stream open the way the daemon does.
 */
function aChangeTail() {
  const pushed: LiveKitRoomsChange[] = [];
  const wakers = new Set<() => void>();
  return {
    push(change: LiveKitRoomsChange) {
      pushed.push(change);
      const waiting = [...wakers];
      wakers.clear();
      for (const wake of waiting) wake();
    },
    async *changes() {
      let cursor = 0;
      for (;;) {
        while (cursor < pushed.length) {
          yield create(LiveKitRoomsEventSchema, {
            event: { case: "change", value: pushed[cursor] },
          });
          cursor += 1;
        }
        await new Promise<void>((resolve) => wakers.add(resolve));
      }
    },
  };
}
