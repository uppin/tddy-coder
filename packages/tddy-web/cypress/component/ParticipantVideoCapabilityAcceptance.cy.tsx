/**
 * Acceptance tests: the participant camera column and its preview dialog exist only on a roster
 * read over a wire that can carry video tracks.
 *
 * A camera track arrives over the same connection the roster does, so a wire with no tracks has no
 * video to preview: the column is absent rather than permanently empty, and the dialog behind it
 * can never be reached. There is no session in scope here — the roster belongs to the host's common
 * room, so it is the host connection that answers.
 *
 * Every absence is asserted next to something that does render, because a `not.exist` on its own is
 * satisfied just as well by a component that threw.
 *
 * Technical: packages/tddy-web/docs/capability-gating.md.
 */

import React from "react";
import { ParticipantList } from "../../src/components/ParticipantList";
import type { RoomParticipant } from "../../src/hooks/useRoomParticipants";
import type { ConnectionCapability } from "../../src/rpc/connections/types";
import type { CapabilityBearing } from "../../src/rpc/connections/useHasCapability";
import { participantListPage as page } from "../support/pages/participantListPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const PEER_WITH_A_CAMERA = "camera-peer";

function aRosterOf(...identities: string[]): RoomParticipant[] {
  return identities.map((identity) => ({
    identity,
    role: "browser" as const,
    joinedAt: 1_700_000_000_000,
    metadata: "",
    codexOAuth: null,
  }));
}

function aConnectionThatCan(...capabilities: ConnectionCapability[]): CapabilityBearing {
  return { capabilities: new Set(capabilities) };
}

/** What a common room joined over LiveKit advertises. */
function aRosterWireThatCarriesTracks(): CapabilityBearing {
  return aConnectionThatCan("rpc", "media", "presence");
}

/** What a roster reached over a frame pipe advertises — a list of names, and no tracks. */
function aRosterWireWithoutTracks(): CapabilityBearing {
  return aConnectionThatCan("rpc", "presence");
}

function mountRosterOver(connection: CapabilityBearing) {
  cy.mount(
    <ParticipantList
      participants={aRosterOf(PEER_WITH_A_CAMERA)}
      roomStatus="connected"
      connectionError={null}
      participantHasCameraVideo={{ [PEER_WITH_A_CAMERA]: true }}
      connection={connection}
    />,
  );
}

// ---------------------------------------------------------------------------
// The camera column
// ---------------------------------------------------------------------------

it("hides the camera column when the roster's wire carries no tracks", () => {
  // Given a participant the roster says has a camera, on a wire that cannot carry one
  // When the roster renders
  mountRosterOver(aRosterWireWithoutTracks());

  // Then the row is listed, and it has no camera column at all
  page.entry(PEER_WITH_A_CAMERA).should("exist");
  page.videoCell(PEER_WITH_A_CAMERA).should("not.exist");
  page.videoTrigger(PEER_WITH_A_CAMERA).should("not.exist");
});

it("offers the camera preview when the roster's wire carries tracks", () => {
  // Given the same participant on a wire that carries tracks
  // When the roster renders
  mountRosterOver(aRosterWireThatCarriesTracks());

  // Then the affordance is offered exactly as it always was
  page.videoTrigger(PEER_WITH_A_CAMERA).should("be.visible");
});

// ---------------------------------------------------------------------------
// The preview dialog
// ---------------------------------------------------------------------------

it("cannot reach the preview dialog when the roster's wire carries no tracks", () => {
  // Given a roster on a wire that cannot carry a camera track
  // When it renders
  mountRosterOver(aRosterWireWithoutTracks());

  // Then the row is there and nothing opens the preview over it
  page.entry(PEER_WITH_A_CAMERA).should("exist");
  page.videoDialog().should("not.exist");
  page.videoPreview().should("not.exist");
});

it("opens the preview dialog from the camera affordance when the wire carries tracks", () => {
  // Given a roster on a wire that carries tracks
  mountRosterOver(aRosterWireThatCarriesTracks());

  // When the operator activates the affordance
  page.videoTrigger(PEER_WITH_A_CAMERA).click();

  // Then the preview opens, unchanged
  page.videoDialog().should("be.visible");
  page.videoPreview().should("be.visible");
});
