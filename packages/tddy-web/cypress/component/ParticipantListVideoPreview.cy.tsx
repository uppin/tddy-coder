import React from "react";
import { ParticipantList, type RoomParticipant } from "tddy-livekit-web";
import { ParticipantVideoPreviewDialog } from "../../src/components/ParticipantVideoPreviewDialog";
import { participantListPage } from "../support/pages/participantListPage";

/**
 * The camera preview the roster opens is a media surface, so the app lends the dialog
 * (`ParticipantList`'s `VideoPreview` slot). These two scenarios stay with the dialog until
 * `feature/web-split/media` moves it behind `MediaPort`; the rest of the roster is in
 * `tddy-livekit-web`'s own `ParticipantList.cy.tsx`.
 */
const A_WIRE_THAT_CARRIES_TRACKS = { capabilities: new Set(["rpc", "media", "presence"] as const) };

function mountParticipantList(props: Omit<React.ComponentProps<typeof ParticipantList>, "VideoPreview">) {
  cy.mount(<ParticipantList {...props} VideoPreview={ParticipantVideoPreviewDialog} />);
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
      participantHasCameraVideo: { "preview-peer": true },
      connection: A_WIRE_THAT_CARRIES_TRACKS,
    });

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
      participantHasCameraVideo: { "cleanup-peer": true },
      connection: A_WIRE_THAT_CARRIES_TRACKS,
    });
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
      participantHasCameraVideo: { "camera-peer": true },
      connection: A_WIRE_THAT_CARRIES_TRACKS,
    });

    // When the operator activates the affordance
    participantListPage.videoTrigger("camera-peer").click();

    // Then the preview opens, unchanged
    participantListPage.videoDialog().should("be.visible");
    participantListPage.videoPreview().should("be.visible");
  });
});
