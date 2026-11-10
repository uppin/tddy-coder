import React from "react";
import type { RoomParticipant } from "../../src";
import { ParticipantVideoPreviewDialog } from "../../src/media/ParticipantVideoPreviewDialog";
import { participantListPage } from "../support/pages/participantListPage";
import { mountRosterWithCameras } from "../support/cameraRoster";

/**
 * The camera preview the roster opens is the media port's dialog; these scenarios mount the LiveKit
 * dialog itself behind the port's `VideoPreview`, with the double saying who has camera video. The
 * rest of the roster is in `ParticipantList.cy.tsx`.
 */
const A_WIRE_THAT_CARRIES_TRACKS = { capabilities: new Set(["rpc", "media", "presence"] as const) };

function mountParticipantList(props: Parameters<typeof mountRosterWithCameras>[0], withCamera: readonly string[]) {
  mountRosterWithCameras(props, withCamera, {
    VideoPreview: ({ identity, open, onOpenChange }) => (
      <ParticipantVideoPreviewDialog identity={identity} open={open} onOpenChange={onOpenChange} />
    ),
  });
}

describe("ParticipantList camera preview", () => {
  it("activating video affordance opens dialog with video preview region", () => {
    // Given
    const participants: RoomParticipant[] = [
      {
        identity: "preview-peer",
        role: "browser",
        joinedAt: 1_700_000_000_000,
        metadata: "",
        codexOAuth: null,
      },
    ];
    mountParticipantList({
      participants,
      roomStatus: "connected",
      connectionError: null,
      connection: A_WIRE_THAT_CARRIES_TRACKS,
    }, ["preview-peer"]);

    // When
    participantListPage.videoTrigger("preview-peer").should("be.visible").click();

    // Then
    participantListPage.videoDialog().should("be.visible");
    participantListPage.videoDialog().should("have.attr", "role", "dialog");
    participantListPage.videoPreview().should("be.visible");
  });

  it("closing the video dialog removes it and the preview from the document", () => {
    // Given
    const participants: RoomParticipant[] = [
      {
        identity: "cleanup-peer",
        role: "browser",
        joinedAt: 1_700_000_000_000,
        metadata: "",
        codexOAuth: null,
      },
    ];
    mountParticipantList({
      participants,
      roomStatus: "connected",
      connectionError: null,
      connection: A_WIRE_THAT_CARRIES_TRACKS,
    }, ["cleanup-peer"]);
    participantListPage.videoTrigger("cleanup-peer").should("be.visible").click();
    participantListPage.videoDialog().should("be.visible");

    // When
    participantListPage.videoDialog().within(() => {
      participantListPage.videoDialogClose().click();
    });

    // Then
    participantListPage.videoDialog().should("not.exist");
    participantListPage.videoPreview().should("not.exist");
  });

  it("opens the preview dialog from the camera affordance when the wire carries tracks", () => {
    // Given a roster on a wire that carries tracks
    mountParticipantList({
      participants: [{ identity: "camera-peer", role: "browser", joinedAt: 1_700_000_000_000, metadata: "", codexOAuth: null }],
      roomStatus: "connected",
      connectionError: null,
      connection: A_WIRE_THAT_CARRIES_TRACKS,
    }, ["camera-peer"]);

    // When the operator activates the affordance
    participantListPage.videoTrigger("camera-peer").click();

    // Then the preview opens, unchanged
    participantListPage.videoDialog().should("be.visible");
    participantListPage.videoPreview().should("be.visible");
  });
});
