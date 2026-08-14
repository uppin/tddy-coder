/**
 * Cypress component acceptance: LiveKit rooms & participants panel.
 *
 * A second panel on `#/livekit`, below the existing connected-participants panel, listing every room
 * on the LiveKit server and the participants joined to each. Fed by
 * `ConnectionService.StreamLiveKitRooms`, whose first message is a full snapshot and whose every
 * later message is one change event. Participant metadata is revealed on pointer-hover or keyboard
 * focus; the harness drives the focus path (see `liveKitRoomsPanelPage.revealMetadata`).
 *
 * PRD: `docs/ft/web/livekit-rooms-panel.md`
 * Changeset: `livekit-rooms-panel`
 */

import React from "react";
import { LiveKitAppPage } from "../../src/components/livekit/LiveKitAppPage";
import { LiveKitRoomsPanel } from "../../src/components/livekit/LiveKitRoomsPanel";
import { TooltipProvider } from "../../src/components/ui/tooltip";
import { mountWithRpc } from "../support/rpc/inMemory";
import { DEFAULT_TEST_DAEMON, withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import {
  aLiveKitRoomsBackend,
  aParticipant,
  aRoom,
  participantJoined,
  participantLeft,
  participantMetadataChanged,
  participantStateChanged,
  roomAdded,
  roomRemoved,
  type LiveKitRoomsBackend,
  type LiveKitRoomsScenario,
} from "../support/rpc/liveKitRoomsBackend";
import { liveKitRoomsPanelPage as panel } from "../support/pages/liveKitRoomsPanelPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const COMMON_ROOM = "livekit.common_room";
const PRESENTER_ROOM = "daemon-pr-stack-presenter-room-0001";

/** A well-formed daemon advertisement — `inferParticipantRole` reads this as role `daemon`. */
const DAEMON_ADVERTISEMENT = '{"instance_id":"workstation","label":"workstation (this daemon)"}';

/** A one-key metadata document, and the exact text the panel's card should show for it. */
const PROJECT_COUNT_METADATA = '{"owned_project_count":3}';
const PROJECT_COUNT_PRETTY = '{\n  "owned_project_count": 3\n}';

const UPDATED_METADATA = '{"owned_project_count":7}';
const UPDATED_PRETTY = '{\n  "owned_project_count": 7\n}';

function aCommonRoomWith(...participants: ReturnType<typeof aParticipant>[]) {
  return aRoom({ name: COMMON_ROOM, participants });
}

function aRoomsBackend(scenario: LiveKitRoomsScenario): LiveKitRoomsBackend {
  return aLiveKitRoomsBackend(scenario);
}

function mountScreen(rooms: LiveKitRoomsBackend) {
  mountWithRpc(
    withSelectedDaemon(<LiveKitAppPage onNavigate={cy.stub()} />, [DEFAULT_TEST_DAEMON], []),
    rooms.backend,
  );
}

/**
 * Deliver change events **in command-queue order**.
 *
 * A bare `rooms.pushChange(...)` is a plain function call, so it would run while the test body is
 * being evaluated — before `cy.mount` has even been dequeued. The change would then already be in
 * the fake's tail when the stream subscribes, arrive as part of the initial snapshot read, and never
 * be a live delta at all. Wrapping the push in `cy.then` puts it where the `// When` it sits under
 * says it is.
 */
function deliver(rooms: LiveKitRoomsBackend, ...changes: Parameters<typeof rooms.pushChange>[0][]) {
  cy.then(() => {
    for (const change of changes) rooms.pushChange(change);
  });
}

// ---------------------------------------------------------------------------
// Unmountable harness
// ---------------------------------------------------------------------------

/** The harness's own control, not part of the panel — hence a local id rather than a `TEST_IDS` one. */
const CLOSE_SCREEN_BUTTON = "close-livekit-screen";

/**
 * The panel has no affordance that unmounts it and `cy.mount` exposes no unmount command, so the
 * one spec about teardown mounts the panel behind a switch it can flip. Everything else mounts the
 * real screen.
 */
function ClosableRoomsScreen() {
  const [open, setOpen] = React.useState(true);
  return (
    <TooltipProvider delayDuration={0}>
      <button type="button" data-testid={CLOSE_SCREEN_BUTTON} onClick={() => setOpen(false)}>
        Close
      </button>
      {open && <LiveKitRoomsPanel />}
    </TooltipProvider>
  );
}

const closableScreen = {
  mount(rooms: LiveKitRoomsBackend) {
    mountWithRpc(
      withSelectedDaemon(<ClosableRoomsScreen />, [DEFAULT_TEST_DAEMON], []),
      rooms.backend,
    );
  },
  close() {
    cy.get(`[data-testid='${CLOSE_SCREEN_BUTTON}']`).click();
  },
};

// ---------------------------------------------------------------------------

describe("LiveKit rooms panel", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  // -------------------------------------------------------------------------
  // Placement and snapshot rendering
  // -------------------------------------------------------------------------

  it("renders the rooms panel below the connected-participants panel", () => {
    // Given a server with one room
    const rooms = aRoomsBackend({ rooms: [aCommonRoomWith(aParticipant({ identity: "browser-alice" }))] });

    // When the LiveKit screen is mounted
    mountScreen(rooms);

    // Then the rooms panel sits after the untouched connected-participants panel
    panel.panel().should("exist");
    panel.expectRenderedBelowConnectedParticipants();
  });

  it("lists one row per room with its name, participant count and creation time", () => {
    // Given a common room with two participants and a presenter room with one
    const rooms = aRoomsBackend({
      rooms: [
        aRoom({
          name: COMMON_ROOM,
          createdAtMs: 1_786_429_800_000,
          participants: [
            aParticipant({ identity: "browser-alice" }),
            aParticipant({ identity: "workstation", metadata: DAEMON_ADVERTISEMENT }),
          ],
        }),
        aRoom({
          name: PRESENTER_ROOM,
          createdAtMs: 1_786_429_800_000,
          participants: [aParticipant({ identity: "daemon-local-sess-1" })],
        }),
      ],
    });

    // When the screen is mounted
    mountScreen(rooms);

    // Then each room reports its own name, count and creation time
    panel.roomName(COMMON_ROOM).should("have.text", COMMON_ROOM);
    panel.roomParticipantCount(COMMON_ROOM).should("have.text", "2");
    panel.roomCreatedAt(COMMON_ROOM).should("have.text", new Date(1_786_429_800_000).toLocaleString());

    panel.roomName(PRESENTER_ROOM).should("have.text", PRESENTER_ROOM);
    panel.roomParticipantCount(PRESENTER_ROOM).should("have.text", "1");
  });

  it("names a room by the label in its metadata", () => {
    // Given a room whose own metadata labels it for a human
    const rooms = aRoomsBackend({
      rooms: [
        aRoom({
          name: PRESENTER_ROOM,
          metadata: '{"label":"PR-stack presenter"}',
          participants: [aParticipant({ identity: "daemon-local-sess-1" })],
        }),
      ],
    });

    // When the screen is mounted
    mountScreen(rooms);

    // Then the label renders beside the opaque room name, which is still shown
    panel.roomLabel(PRESENTER_ROOM).should("have.text", "PR-stack presenter");
    panel.roomName(PRESENTER_ROOM).should("have.text", PRESENTER_ROOM);
  });

  it("shows no label for a room whose metadata carries none", () => {
    // Given a room with no metadata at all — the realistic case, since nothing publishes it today
    const rooms = aRoomsBackend({
      rooms: [aCommonRoomWith(aParticipant({ identity: "browser-alice" }))],
    });

    // When the screen is mounted
    mountScreen(rooms);

    // Then the row renders without a label element, rather than an empty or placeholder one
    panel.roomName(COMMON_ROOM).should("have.text", COMMON_ROOM);
    panel.roomLabel(COMMON_ROOM, { timeout: 1000 }).should("not.exist");
  });

  it("lists a room's participants with identity, joined time and server state", () => {
    // Given one participant that joined at a known instant, in state JOINED
    const rooms = aRoomsBackend({
      rooms: [
        aCommonRoomWith(
          aParticipant({
            identity: "browser-alice",
            joinedAtMs: 1_786_431_600_000,
            state: "JOINED",
          }),
        ),
      ],
    });

    // When the screen is mounted
    mountScreen(rooms);

    // Then the participant row carries all three facts
    panel.participant(COMMON_ROOM, "browser-alice").should("contain.text", "browser-alice");
    panel
      .participantJoined(COMMON_ROOM, "browser-alice")
      .should("have.text", new Date(1_786_431_600_000).toLocaleString());
    panel.participantState(COMMON_ROOM, "browser-alice").should("have.text", "JOINED");
  });

  it("labels each participant with the role its identity and metadata imply", () => {
    // Given a browser, a coder session and a daemon advertisement in one room
    const rooms = aRoomsBackend({
      rooms: [
        aCommonRoomWith(
          aParticipant({ identity: "browser-alice" }),
          aParticipant({ identity: "daemon-local-sess-1" }),
          aParticipant({ identity: "workstation", metadata: DAEMON_ADVERTISEMENT }),
        ),
      ],
    });

    // When the screen is mounted
    mountScreen(rooms);

    // Then each row is labelled by the shared inference grammar
    panel.participantRole(COMMON_ROOM, "browser-alice").should("have.text", "browser");
    panel.participantRole(COMMON_ROOM, "daemon-local-sess-1").should("have.text", "coder");
    panel.participantRole(COMMON_ROOM, "workstation").should("have.text", "daemon");
  });

  it("renders rooms in name order regardless of the order the snapshot listed them", () => {
    // Given a snapshot listing the common room after the presenter room's name-successor
    const rooms = aRoomsBackend({
      rooms: [
        aRoom({ name: PRESENTER_ROOM, participants: [aParticipant({ identity: "alpha" })] }),
        aCommonRoomWith(aParticipant({ identity: "browser-alice" })),
      ],
    });

    // When the screen is mounted
    mountScreen(rooms);

    // Then — "daemon-…" sorts before "livekit.…", so the presenter room renders first
    panel.expectRoomsInOrder([PRESENTER_ROOM, COMMON_ROOM]);
  });

  it("renders a room's participants in identity order regardless of the order they arrived", () => {
    // Given one room whose snapshot lists "zeta" ahead of "alpha"
    const rooms = aRoomsBackend({
      rooms: [
        aRoom({
          name: PRESENTER_ROOM,
          participants: [
            aParticipant({ identity: "zeta" }),
            aParticipant({ identity: "alpha" }),
          ],
        }),
      ],
    });

    // When the screen is mounted
    mountScreen(rooms);

    // Then the rows render identity-sorted
    panel.expectParticipantsInOrder(PRESENTER_ROOM, ["alpha", "zeta"]);
  });

  it("hides a room's participants when the row is collapsed", () => {
    // Given a room rendering its participants
    const rooms = aRoomsBackend({
      rooms: [aCommonRoomWith(aParticipant({ identity: "browser-alice" }))],
    });
    mountScreen(rooms);
    panel.participant(COMMON_ROOM, "browser-alice").should("exist");

    // When the room row is collapsed
    panel.toggleRoom(COMMON_ROOM);

    // Then its participants are gone while the room row remains
    panel.participant(COMMON_ROOM, "browser-alice", { timeout: 1000 }).should("not.exist");
    panel.room(COMMON_ROOM).should("exist");
  });

  it("expands a room recreated under a name that was collapsed before it closed", () => {
    // Given a presenter room that was collapsed and has since closed
    const rooms = aRoomsBackend({
      rooms: [
        aCommonRoomWith(aParticipant({ identity: "browser-alice" })),
        aRoom({
          name: PRESENTER_ROOM,
          participants: [aParticipant({ identity: "daemon-local-sess-1" })],
        }),
      ],
    });
    mountScreen(rooms);
    panel.toggleRoom(PRESENTER_ROOM);
    panel.participant(PRESENTER_ROOM, "daemon-local-sess-1", { timeout: 1000 }).should("not.exist");
    deliver(rooms, roomRemoved(PRESENTER_ROOM));
    panel.room(PRESENTER_ROOM, { timeout: 1000 }).should("not.exist");

    // When a room of that same name opens again
    deliver(
      rooms,
      roomAdded(
        aRoom({
          name: PRESENTER_ROOM,
          participants: [aParticipant({ identity: "daemon-local-sess-2" })],
        }),
      ),
    );

    // Then it renders expanded, like every other room the panel has just been told about
    panel.participant(PRESENTER_ROOM, "daemon-local-sess-2").should("exist");
  });

  // -------------------------------------------------------------------------
  // Metadata card
  // -------------------------------------------------------------------------

  it("reveals a participant's pretty-printed metadata when the row takes focus", () => {
    // Given a participant carrying a one-key metadata document
    const rooms = aRoomsBackend({
      rooms: [
        aCommonRoomWith(
          aParticipant({ identity: "workstation", metadata: PROJECT_COUNT_METADATA }),
        ),
      ],
    });
    mountScreen(rooms);

    // When its row takes focus
    panel.revealMetadata(COMMON_ROOM, "workstation");

    // Then the card shows that document, pretty-printed
    panel.expectMetadataCard(COMMON_ROOM, "workstation", PROJECT_COUNT_PRETTY);
  });

  it("states that no metadata is published for a participant that published none", () => {
    // Given a participant with an empty metadata string
    const rooms = aRoomsBackend({
      rooms: [aCommonRoomWith(aParticipant({ identity: "browser-alice", metadata: "" }))],
    });
    mountScreen(rooms);

    // When its row takes focus
    panel.revealMetadata(COMMON_ROOM, "browser-alice");

    // Then the card says so, rather than not appearing at all
    panel.expectMetadataCard(COMMON_ROOM, "browser-alice", "No metadata published.");
  });

  it("shows metadata that is not valid JSON verbatim", () => {
    // Given a participant whose metadata is not a JSON document
    const rooms = aRoomsBackend({
      rooms: [aCommonRoomWith(aParticipant({ identity: "browser-alice", metadata: "not-json" }))],
    });
    mountScreen(rooms);

    // When its row takes focus
    panel.revealMetadata(COMMON_ROOM, "browser-alice");

    // Then the string is relayed as published rather than dropped
    panel.expectMetadataCard(COMMON_ROOM, "browser-alice", "not-json");
  });

  // -------------------------------------------------------------------------
  // Change events folded onto the snapshot
  // -------------------------------------------------------------------------

  it("adds a joining participant to its room", () => {
    // Given a common room holding one participant
    const rooms = aRoomsBackend({
      rooms: [aCommonRoomWith(aParticipant({ identity: "browser-alice" }))],
    });
    mountScreen(rooms);
    panel.roomParticipantCount(COMMON_ROOM).should("have.text", "1");

    // When a second participant joins
    deliver(rooms, participantJoined(COMMON_ROOM, aParticipant({ identity: "browser-bob" })));

    // Then it appears in that room and the count follows
    panel.participant(COMMON_ROOM, "browser-bob").should("exist");
    panel.roomParticipantCount(COMMON_ROOM).should("have.text", "2");
  });

  it("removes a leaving participant and decrements its room's count", () => {
    // Given a common room holding two participants
    const rooms = aRoomsBackend({
      rooms: [
        aCommonRoomWith(
          aParticipant({ identity: "browser-alice" }),
          aParticipant({ identity: "browser-bob" }),
        ),
      ],
    });
    mountScreen(rooms);
    panel.roomParticipantCount(COMMON_ROOM).should("have.text", "2");

    // When one leaves
    deliver(rooms, participantLeft(COMMON_ROOM, "browser-bob"));

    // Then its row is gone and the count follows
    panel.participant(COMMON_ROOM, "browser-bob", { timeout: 1000 }).should("not.exist");
    panel.roomParticipantCount(COMMON_ROOM).should("have.text", "1");
    panel.participant(COMMON_ROOM, "browser-alice").should("exist");
  });

  it("adds a room with the participants already in it", () => {
    // Given a server with only the common room
    const rooms = aRoomsBackend({
      rooms: [aCommonRoomWith(aParticipant({ identity: "browser-alice" }))],
    });
    mountScreen(rooms);
    panel.room(COMMON_ROOM).should("exist");

    // When a presenter room appears, already occupied
    deliver(
      rooms,
      roomAdded(
        aRoom({
          name: PRESENTER_ROOM,
          participants: [aParticipant({ identity: "daemon-local-sess-1" })],
        }),
      ),
    );

    // Then the room and its occupant both render
    panel.roomName(PRESENTER_ROOM).should("have.text", PRESENTER_ROOM);
    panel.roomParticipantCount(PRESENTER_ROOM).should("have.text", "1");
    panel.participant(PRESENTER_ROOM, "daemon-local-sess-1").should("exist");
  });

  it("removes a room that closed", () => {
    // Given two rooms
    const rooms = aRoomsBackend({
      rooms: [
        aCommonRoomWith(aParticipant({ identity: "browser-alice" })),
        aRoom({ name: PRESENTER_ROOM, participants: [aParticipant({ identity: "daemon-local-sess-1" })] }),
      ],
    });
    mountScreen(rooms);
    panel.room(PRESENTER_ROOM).should("exist");

    // When the presenter room closes
    deliver(rooms, roomRemoved(PRESENTER_ROOM));

    // Then it is gone and the common room is untouched
    panel.room(PRESENTER_ROOM, { timeout: 1000 }).should("not.exist");
    panel.room(COMMON_ROOM).should("exist");
  });

  it("updates a participant's metadata card when its metadata is republished", () => {
    // Given a participant whose card shows a project count of 3
    const rooms = aRoomsBackend({
      rooms: [
        aCommonRoomWith(
          aParticipant({ identity: "workstation", metadata: PROJECT_COUNT_METADATA }),
        ),
      ],
    });
    mountScreen(rooms);
    panel.revealMetadata(COMMON_ROOM, "workstation");
    panel.expectMetadataCard(COMMON_ROOM, "workstation", PROJECT_COUNT_PRETTY);

    // When the publisher republishes it
    deliver(rooms, participantMetadataChanged(COMMON_ROOM, "workstation", UPDATED_METADATA));

    // Then the card shows the new document
    panel.revealMetadata(COMMON_ROOM, "workstation");
    panel.expectMetadataCard(COMMON_ROOM, "workstation", UPDATED_PRETTY);
  });

  it("updates a participant's state cell when the server reports it settled", () => {
    // Given a participant whose row shows the state it was first seen in
    const rooms = aRoomsBackend({
      rooms: [aCommonRoomWith(aParticipant({ identity: "browser-alice", state: "JOINED" }))],
    });
    mountScreen(rooms);
    panel.participantState(COMMON_ROOM, "browser-alice").should("have.text", "JOINED");

    // When its connection comes up
    deliver(rooms, participantStateChanged(COMMON_ROOM, "browser-alice", "ACTIVE"));

    // Then the cell follows, without waiting for a fresh subscription
    panel.participantState(COMMON_ROOM, "browser-alice").should("have.text", "ACTIVE");
  });

  it("ignores a change event naming a room it does not know", () => {
    // Given a server with only the common room
    const rooms = aRoomsBackend({
      rooms: [aCommonRoomWith(aParticipant({ identity: "browser-alice" }))],
    });
    mountScreen(rooms);
    panel.roomParticipantCount(COMMON_ROOM).should("have.text", "1");

    // When someone joins a room the panel has never been told about, followed by a join the panel
    // *can* place — the second one rendering is what proves the first was delivered and dropped,
    // rather than never having reached the client at all
    deliver(
      rooms,
      participantJoined("room-never-announced", aParticipant({ identity: "ghost" })),
      participantJoined(COMMON_ROOM, aParticipant({ identity: "browser-bob" })),
    );
    panel.participant(COMMON_ROOM, "browser-bob").should("exist");

    // Then no room was conjured from the partial event and the known room holds only its own
    panel.room("room-never-announced", { timeout: 1000 }).should("not.exist");
    panel.panel().should("not.contain.text", "ghost");
    panel.expectParticipantsInOrder(COMMON_ROOM, ["browser-alice", "browser-bob"]);
  });

  // -------------------------------------------------------------------------
  // Empty and error states
  // -------------------------------------------------------------------------

  it("says the server has no rooms when the snapshot is empty", () => {
    // Given a LiveKit server with no rooms at all
    const rooms = aRoomsBackend({ rooms: [] });

    // When the screen is mounted
    mountScreen(rooms);

    // Then the panel says so
    panel.empty().should("be.visible").and("contain.text", "No rooms on the LiveKit server.");
  });

  it("says a known room has no participants when it is empty", () => {
    // Given a room nobody is joined to
    const rooms = aRoomsBackend({ rooms: [aRoom({ name: PRESENTER_ROOM, participants: [] })] });

    // When the screen is mounted
    mountScreen(rooms);

    // Then the room still renders, with a zero count and an explicit empty note
    panel.roomParticipantCount(PRESENTER_ROOM).should("have.text", "0");
    panel.roomNoParticipants(PRESENTER_ROOM).should("contain.text", "No participants joined.");
  });

  it("shows the daemon's error when the stream fails before any snapshot", () => {
    // Given a daemon that cannot reach the LiveKit server API
    const rooms = aRoomsBackend({
      rooms: [],
      failBeforeSnapshot: "livekit room service unreachable",
    });

    // When the screen is mounted
    mountScreen(rooms);

    // Then the panel quotes the reason instead of sitting on its loading placeholder
    panel.error().should("contain.text", "livekit room service unreachable");
    panel.loading({ timeout: 1000 }).should("not.exist");
  });

  it("keeps the last-known rooms visible when the stream fails after a snapshot", () => {
    // Given a stream that delivers a snapshot and then drops with an error
    const rooms = aRoomsBackend({
      rooms: [aCommonRoomWith(aParticipant({ identity: "browser-alice" }))],
      failAfterSnapshot: "livekit room service unreachable",
    });

    // When the screen is mounted
    mountScreen(rooms);

    // Then the error is shown *alongside* the roster it already had
    panel.error().should("contain.text", "livekit room service unreachable");
    panel.room(COMMON_ROOM).should("exist");
    panel.participant(COMMON_ROOM, "browser-alice").should("exist");
  });

  // -------------------------------------------------------------------------
  // Subscription
  // -------------------------------------------------------------------------

  it("opens exactly one rooms subscription for the screen", () => {
    // Given a server with two rooms
    const rooms = aRoomsBackend({
      rooms: [
        aCommonRoomWith(aParticipant({ identity: "browser-alice" })),
        aRoom({ name: PRESENTER_ROOM, participants: [aParticipant({ identity: "daemon-local-sess-1" })] }),
      ],
    });

    // When the screen is mounted and both rooms have rendered
    mountScreen(rooms);
    panel.room(COMMON_ROOM).should("exist");
    panel.room(PRESENTER_ROOM).should("exist");

    // Then the panel opened one feed, not one per room
    cy.wrap(null).should(() => {
      expect(rooms.roomsStreamCount()).to.equal(1);
    });
  });

  it("cancels the rooms subscription when the panel unmounts", () => {
    // Given a panel tailing the feed, with nothing further to say
    const rooms = aRoomsBackend({
      rooms: [aCommonRoomWith(aParticipant({ identity: "browser-alice" }))],
    });
    closableScreen.mount(rooms);
    panel.participant(COMMON_ROOM, "browser-alice").should("exist");

    // When the screen closes and the panel unmounts
    closableScreen.close();
    panel.panel({ timeout: 1000 }).should("not.exist");

    // Then the call is cancelled — an idle feed sends nothing, so a subscription left open here
    // would stay parked for the life of the tab
    cy.wrap(null).should(() => {
      expect(rooms.cancelledRoomsStreamCount()).to.equal(1);
    });
  });
});
