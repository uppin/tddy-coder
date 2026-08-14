import { describe, it, expect } from "bun:test";
import { create } from "@bufbuild/protobuf";
import {
  LiveKitParticipantInfoSchema,
  LiveKitRoomInfoSchema,
  LiveKitRoomsChangeSchema,
  type LiveKitParticipantInfo,
  type LiveKitRoomInfo,
} from "../gen/connection_pb";
import {
  applyRoomsChange,
  roomLabelFromMetadata,
  roomsFromSnapshot,
  type LiveKitRoom,
} from "./liveKitRoomsState";

/**
 * The rooms panel folds a snapshot plus a stream of single-delta change events into its state. These
 * pin the fold itself: what a snapshot becomes, what each of the six change kinds does to it, and
 * what happens to a change the client cannot place. The panel's acceptance specs assert the rendered
 * result; these assert the arithmetic behind it, where a wrong count or a dropped room is a one-line
 * failure rather than a missing DOM node.
 */

const COMMON_ROOM = "livekit.common_room";
const PRESENTER_ROOM = "daemon-pr-stack-presenter-room-0001";

const JOINED_AT = 1_786_431_600_000;
const CREATED_AT = 1_786_429_800_000;

function aParticipantInfo(
  overrides: { identity: string; metadata?: string; state?: string; joinedAtMs?: number },
): LiveKitParticipantInfo {
  return create(LiveKitParticipantInfoSchema, {
    identity: overrides.identity,
    name: "",
    metadata: overrides.metadata ?? "",
    joinedAtMs: BigInt(overrides.joinedAtMs ?? JOINED_AT),
    state: overrides.state ?? "ACTIVE",
  });
}

function aRoomInfo(
  overrides: { name: string; participants?: LiveKitParticipantInfo[]; metadata?: string },
): LiveKitRoomInfo {
  return create(LiveKitRoomInfoSchema, {
    name: overrides.name,
    createdAtMs: BigInt(CREATED_AT),
    participants: overrides.participants ?? [],
    metadata: overrides.metadata ?? "",
  });
}

const identitiesIn = (rooms: LiveKitRoom[], room: string): string[] =>
  rooms.find((r) => r.name === room)?.participants.map((p) => p.identity) ?? [];

const roomNames = (rooms: LiveKitRoom[]): string[] => rooms.map((r) => r.name);

// ---------------------------------------------------------------------------

describe("roomLabelFromMetadata", () => {
  it("reads the label out of a room metadata document", () => {
    // Given / When
    const label = roomLabelFromMetadata('{"label":"PR-stack presenter"}');

    // Then
    expect(label).toEqual("PR-stack presenter");
  });

  it("returns null for metadata that carries no label", () => {
    // Given / When
    const label = roomLabelFromMetadata('{"other":"value"}');

    // Then
    expect(label).toBeNull();
  });

  it("returns null for a blank label", () => {
    // Given / When
    const label = roomLabelFromMetadata('{"label":"   "}');

    // Then
    expect(label).toBeNull();
  });

  it("returns null for an empty metadata string", () => {
    // Given / When
    const label = roomLabelFromMetadata("");

    // Then
    expect(label).toBeNull();
  });

  it("returns null for metadata that is not JSON", () => {
    // Given / When
    const label = roomLabelFromMetadata("not-json");

    // Then
    expect(label).toBeNull();
  });
});

describe("roomsFromSnapshot", () => {
  it("orders rooms by name and each room's participants by identity", () => {
    // Given a snapshot delivered in neither order
    const snapshot = [
      aRoomInfo({
        name: PRESENTER_ROOM,
        participants: [aParticipantInfo({ identity: "zeta" }), aParticipantInfo({ identity: "alpha" })],
      }),
      aRoomInfo({ name: COMMON_ROOM, participants: [aParticipantInfo({ identity: "browser-alice" })] }),
    ];

    // When
    const rooms = roomsFromSnapshot(snapshot);

    // Then — "daemon-…" sorts before "livekit.…", so the presenter room leads
    expect(roomNames(rooms)).toEqual([PRESENTER_ROOM, COMMON_ROOM]);
    expect(identitiesIn(rooms, PRESENTER_ROOM)).toEqual(["alpha", "zeta"]);
  });

  it("carries each participant's server state and join time through unchanged", () => {
    // Given one participant in state JOINED
    const snapshot = [
      aRoomInfo({
        name: COMMON_ROOM,
        participants: [
          aParticipantInfo({ identity: "browser-alice", state: "JOINED", joinedAtMs: JOINED_AT }),
        ],
      }),
    ];

    // When
    const rooms = roomsFromSnapshot(snapshot);

    // Then
    expect(rooms[0].participants[0].state).toEqual("JOINED");
    expect(rooms[0].participants[0].joinedAtMs).toEqual(JOINED_AT);
  });

  it("derives each participant's role from its identity and metadata", () => {
    // Given a browser, a coder session, and a daemon advertisement
    const snapshot = [
      aRoomInfo({
        name: COMMON_ROOM,
        participants: [
          aParticipantInfo({ identity: "browser-alice" }),
          aParticipantInfo({ identity: "daemon-local-sess-1" }),
          aParticipantInfo({
            identity: "workstation",
            metadata: '{"instance_id":"workstation","label":"workstation (this daemon)"}',
          }),
        ],
      }),
    ];

    // When
    const rooms = roomsFromSnapshot(snapshot);

    // Then
    const roles = rooms[0].participants.map((p) => `${p.identity}=${p.role}`);
    expect(roles).toEqual([
      "browser-alice=browser",
      "daemon-local-sess-1=coder",
      "workstation=daemon",
    ]);
  });

  it("resolves each room's label from its own metadata", () => {
    // Given one labelled and one unlabelled room
    const snapshot = [
      aRoomInfo({ name: COMMON_ROOM, metadata: "" }),
      aRoomInfo({ name: PRESENTER_ROOM, metadata: '{"label":"PR-stack presenter"}' }),
    ];

    // When
    const rooms = roomsFromSnapshot(snapshot);

    // Then
    expect(rooms.find((r) => r.name === COMMON_ROOM)?.label).toBeNull();
    expect(rooms.find((r) => r.name === PRESENTER_ROOM)?.label).toEqual("PR-stack presenter");
  });

  it("returns no rooms for an empty snapshot", () => {
    // Given / When
    const rooms = roomsFromSnapshot([]);

    // Then
    expect(rooms).toEqual([]);
  });
});

describe("applyRoomsChange", () => {
  const oneCommonRoom = (): LiveKitRoom[] =>
    roomsFromSnapshot([
      aRoomInfo({ name: COMMON_ROOM, participants: [aParticipantInfo({ identity: "browser-alice" })] }),
    ]);

  it("adds a joining participant in identity order", () => {
    // Given a room holding "browser-alice"
    const before = oneCommonRoom();

    // When "browser-aaron" joins, sorting before her
    const after = applyRoomsChange(
      before,
      create(LiveKitRoomsChangeSchema, {
        change: {
          case: "participantJoined",
          value: { room: COMMON_ROOM, participant: aParticipantInfo({ identity: "browser-aaron" }) },
        },
      }),
    );

    // Then
    expect(identitiesIn(after, COMMON_ROOM)).toEqual(["browser-aaron", "browser-alice"]);
  });

  it("removes a leaving participant", () => {
    // Given a room holding two participants
    const before = roomsFromSnapshot([
      aRoomInfo({
        name: COMMON_ROOM,
        participants: [
          aParticipantInfo({ identity: "browser-alice" }),
          aParticipantInfo({ identity: "browser-bob" }),
        ],
      }),
    ]);

    // When one leaves
    const after = applyRoomsChange(
      before,
      create(LiveKitRoomsChangeSchema, {
        change: { case: "participantLeft", value: { room: COMMON_ROOM, identity: "browser-bob" } },
      }),
    );

    // Then
    expect(identitiesIn(after, COMMON_ROOM)).toEqual(["browser-alice"]);
  });

  it("adds a room with the participants already in it", () => {
    // Given only the common room
    const before = oneCommonRoom();

    // When a presenter room appears, already occupied
    const after = applyRoomsChange(
      before,
      create(LiveKitRoomsChangeSchema, {
        change: {
          case: "roomAdded",
          value: {
            room: aRoomInfo({
              name: PRESENTER_ROOM,
              participants: [aParticipantInfo({ identity: "daemon-local-sess-1" })],
            }),
          },
        },
      }),
    );

    // Then the new room is present, name-ordered ahead of the common room, carrying its occupant
    expect(roomNames(after)).toEqual([PRESENTER_ROOM, COMMON_ROOM]);
    expect(identitiesIn(after, PRESENTER_ROOM)).toEqual(["daemon-local-sess-1"]);
  });

  it("removes a room that closed", () => {
    // Given two rooms
    const before = applyRoomsChange(
      oneCommonRoom(),
      create(LiveKitRoomsChangeSchema, {
        change: { case: "roomAdded", value: { room: aRoomInfo({ name: PRESENTER_ROOM }) } },
      }),
    );

    // When the presenter room closes
    const after = applyRoomsChange(
      before,
      create(LiveKitRoomsChangeSchema, {
        change: { case: "roomRemoved", value: { room: PRESENTER_ROOM } },
      }),
    );

    // Then
    expect(roomNames(after)).toEqual([COMMON_ROOM]);
  });

  it("replaces a participant's metadata when it is republished", () => {
    // Given a participant carrying a project count of 3
    const before = roomsFromSnapshot([
      aRoomInfo({
        name: COMMON_ROOM,
        participants: [
          aParticipantInfo({ identity: "workstation", metadata: '{"owned_project_count":3}' }),
        ],
      }),
    ]);

    // When the publisher republishes it
    const after = applyRoomsChange(
      before,
      create(LiveKitRoomsChangeSchema, {
        change: {
          case: "participantMetadataChanged",
          value: {
            room: COMMON_ROOM,
            identity: "workstation",
            metadata: '{"owned_project_count":7}',
          },
        },
      }),
    );

    // Then
    expect(after[0].participants[0].metadata).toEqual('{"owned_project_count":7}');
  });

  it("re-derives a participant's role when its metadata makes it a daemon", () => {
    // Given a participant whose identity alone reads as `unknown`
    const before = roomsFromSnapshot([
      aRoomInfo({ name: COMMON_ROOM, participants: [aParticipantInfo({ identity: "workstation" })] }),
    ]);
    expect(before[0].participants[0].role).toEqual("unknown");

    // When it publishes a daemon advertisement
    const after = applyRoomsChange(
      before,
      create(LiveKitRoomsChangeSchema, {
        change: {
          case: "participantMetadataChanged",
          value: {
            room: COMMON_ROOM,
            identity: "workstation",
            metadata: '{"instance_id":"workstation","label":"workstation (this daemon)"}',
          },
        },
      }),
    );

    // Then the row's role follows the new metadata
    expect(after[0].participants[0].role).toEqual("daemon");
  });

  it("replaces a participant's state when the server reports it settled", () => {
    // Given a participant the panel first saw in state JOINED
    const before = roomsFromSnapshot([
      aRoomInfo({
        name: COMMON_ROOM,
        participants: [aParticipantInfo({ identity: "browser-alice", state: "JOINED" })],
      }),
    ]);

    // When its connection comes up
    const after = applyRoomsChange(
      before,
      create(LiveKitRoomsChangeSchema, {
        change: {
          case: "participantStateChanged",
          value: { room: COMMON_ROOM, identity: "browser-alice", state: "ACTIVE" },
        },
      }),
    );

    // Then
    expect(after[0].participants[0].state).toEqual("ACTIVE");
  });

  it("ignores a join naming a room it does not know", () => {
    // Given only the common room
    const before = oneCommonRoom();

    // When someone joins a room that was never announced
    const after = applyRoomsChange(
      before,
      create(LiveKitRoomsChangeSchema, {
        change: {
          case: "participantJoined",
          value: { room: "room-never-announced", participant: aParticipantInfo({ identity: "ghost" }) },
        },
      }),
    );

    // Then no room is conjured and the known room is untouched
    expect(roomNames(after)).toEqual([COMMON_ROOM]);
    expect(identitiesIn(after, COMMON_ROOM)).toEqual(["browser-alice"]);
  });

  it("ignores a metadata change naming a participant it does not know", () => {
    // Given a room holding only "browser-alice"
    const before = oneCommonRoom();

    // When a stranger's metadata changes
    const after = applyRoomsChange(
      before,
      create(LiveKitRoomsChangeSchema, {
        change: {
          case: "participantMetadataChanged",
          value: { room: COMMON_ROOM, identity: "stranger", metadata: '{"label":"x"}' },
        },
      }),
    );

    // Then the roster is unchanged — no placeholder row appears
    expect(identitiesIn(after, COMMON_ROOM)).toEqual(["browser-alice"]);
  });

  it("ignores a state change naming a room it does not know", () => {
    // Given only the common room, holding "browser-alice" in state ACTIVE
    const before = oneCommonRoom();

    // When a participant of a room that was never announced settles
    const after = applyRoomsChange(
      before,
      create(LiveKitRoomsChangeSchema, {
        change: {
          case: "participantStateChanged",
          value: { room: "room-never-announced", identity: "browser-alice", state: "JOINED" },
        },
      }),
    );

    // Then no room is conjured and the known room's participant keeps its own state
    expect(roomNames(after)).toEqual([COMMON_ROOM]);
    expect(after[0].participants[0].state).toEqual("ACTIVE");
  });

  it("ignores a state change naming a participant it does not know", () => {
    // Given a room holding only "browser-alice"
    const before = oneCommonRoom();

    // When a stranger's state changes
    const after = applyRoomsChange(
      before,
      create(LiveKitRoomsChangeSchema, {
        change: {
          case: "participantStateChanged",
          value: { room: COMMON_ROOM, identity: "stranger", state: "ACTIVE" },
        },
      }),
    );

    // Then the roster is unchanged — no placeholder row appears
    expect(identitiesIn(after, COMMON_ROOM)).toEqual(["browser-alice"]);
  });

  it("leaves the previous state untouched when folding a change", () => {
    // Given a room holding one participant
    const before = oneCommonRoom();

    // When a second joins
    applyRoomsChange(
      before,
      create(LiveKitRoomsChangeSchema, {
        change: {
          case: "participantJoined",
          value: { room: COMMON_ROOM, participant: aParticipantInfo({ identity: "browser-bob" }) },
        },
      }),
    );

    // Then the array the caller passed in is not mutated — React reads it by reference
    expect(identitiesIn(before, COMMON_ROOM)).toEqual(["browser-alice"]);
  });
});
