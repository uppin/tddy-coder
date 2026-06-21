import React, { useState } from "react";
import { ParticipantList } from "../../src/components/ParticipantList";
import type { RoomParticipant } from "../../src/hooks/useRoomParticipants";
import { participantListPage } from "../support/pages/participantListPage";
import { byTestId } from "../support/testIds";

/** Canonical LiveKit metadata key (see `OWNED_PROJECT_COUNT_METADATA_KEY` in tddy-livekit). */
const OWNED_PROJECT_COUNT_KEY = "owned_project_count";

function mountParticipantList(props: React.ComponentProps<typeof ParticipantList>) {
  cy.mount(<ParticipantList {...props} />);
}

describe("ParticipantList", () => {
  it("shows connecting state", () => {
    // Given / When
    cy.mount(
      <ParticipantList participants={[]} roomStatus="connecting" connectionError={null} />
    );

    // Then
    participantListPage.list().should("have.attr", "data-room-status", "connecting");
    cy.contains("Connecting to presence room");
  });

  it("shows error state", () => {
    // Given / When
    cy.mount(
      <ParticipantList participants={[]} roomStatus="error" connectionError="token failed" />
    );

    // Then
    participantListPage.list().should("have.attr", "data-room-status", "error");
    participantListPage.error().should("contain.text", "token failed");
  });

  it("shows empty state when connected with no participants", () => {
    // Given / When
    cy.mount(
      <ParticipantList participants={[]} roomStatus="connected" connectionError={null} />
    );

    // Then
    participantListPage.empty().should("exist");
  });

  it("renders participant rows with role badges", () => {
    // Given
    const participants: RoomParticipant[] = [
      {
        identity: "web-testuser",
        role: "browser",
        joinedAt: 1_700_000_000_000,
        metadata: "",
        codexOAuth: null,
      },
      {
        identity: "server-abc",
        role: "coder",
        joinedAt: 1_700_000_000_000,
        metadata: '{"k":"v"}',
        codexOAuth: null,
      },
    ];

    // When
    cy.mount(
      <ParticipantList participants={participants} roomStatus="connected" connectionError={null} />
    );

    // Then
    participantListPage.entry("web-testuser").should("contain.text", "web-testuser");
    participantListPage.role("web-testuser").should("contain.text", "browser");
    participantListPage.role("server-abc").should("contain.text", "coder");
    participantListPage.metadata("server-abc").should("contain.text", '{"k":"v"}');
  });

  it("labels common-room daemon advertisement as daemon", () => {
    // Given
    const meta = JSON.stringify({
      instance_id: "my-host",
      label: "my-host (this daemon)",
    });
    const participants: RoomParticipant[] = [
      {
        identity: "my-host",
        role: "daemon",
        joinedAt: 1_700_000_000_000,
        metadata: meta,
        codexOAuth: null,
      },
    ];

    // When
    cy.mount(
      <ParticipantList participants={participants} roomStatus="connected" connectionError={null} />,
    );

    // Then
    participantListPage.role("my-host").should("contain.text", "daemon");
  });

  it("shows Codex OAuth sign-in link when participant metadata requests it", () => {
    // Given
    const meta = JSON.stringify({
      codex_oauth: {
        pending: true,
        authorize_url: "https://auth.example.com/oauth/authorize",
      },
    });
    const participants: RoomParticipant[] = [
      {
        identity: "daemon-session",
        role: "coder",
        joinedAt: 1_700_000_000_000,
        metadata: meta,
        codexOAuth: null,
      },
    ];

    // When
    cy.mount(
      <ParticipantList participants={participants} roomStatus="connected" connectionError={null} />
    );

    // Then
    participantListPage.codexOauth("daemon-session")
      .find("a")
      .should("have.attr", "href", "https://auth.example.com/oauth/authorize")
      .should("have.attr", "target", "_blank");
    participantListPage.metadata("daemon-session")
      .find("a")
      .should("have.attr", "href", "https://auth.example.com/oauth/authorize")
      .should("contain.text", "open sign-in");
  });

  it("hides video affordance for participants without a camera track", () => {
    // Given
    const withCam: RoomParticipant = {
      identity: "with-cam-user",
      role: "browser",
      joinedAt: 1_700_000_000_000,
      metadata: "",
      codexOAuth: null,
    };
    const noCam: RoomParticipant = {
      identity: "no-cam-user",
      role: "browser",
      joinedAt: 1_700_000_000_000,
      metadata: "",
      codexOAuth: null,
    };

    // When
    mountParticipantList({
      participants: [withCam, noCam],
      roomStatus: "connected",
      connectionError: null,
      participantHasCameraVideo: { "with-cam-user": true, "no-cam-user": false },
    });

    // Then
    participantListPage.videoTrigger("with-cam-user").should("be.visible");
    participantListPage.videoTrigger("no-cam-user").should("not.exist");
  });

  it("shows video affordance with accessible label when participant exposes camera video", () => {
    // Given
    const participants: RoomParticipant[] = [
      {
        identity: "camera-peer",
        role: "browser",
        joinedAt: 1_700_000_000_000,
        metadata: "",
        codexOAuth: null,
      },
    ];

    // When
    mountParticipantList({
      participants,
      roomStatus: "connected",
      connectionError: null,
      participantHasCameraVideo: { "camera-peer": true },
    });

    // Then
    participantListPage.videoTrigger("camera-peer").should("be.visible");
    participantListPage.videoTrigger("camera-peer")
      .should("have.attr", "aria-label")
      .and("match", /camera-peer/i);
  });

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

  it("renders owned-project count badge for a participant", () => {
    // Given
    const count = 7;
    const meta = JSON.stringify({ [OWNED_PROJECT_COUNT_KEY]: count });
    const participants: RoomParticipant[] = [
      {
        identity: "server-agent",
        role: "coder",
        joinedAt: 1_700_000_000_000,
        metadata: meta,
        codexOAuth: null,
      },
    ];

    // When
    cy.mount(
      <ParticipantList participants={participants} roomStatus="connected" connectionError={null} />,
    );

    // Then
    participantListPage.ownedProjectCount("server-agent").should("have.text", String(count));
  });

  it("updates the owned-project count badge when participant metadata changes", () => {
    // Given — harness component that can push new metadata
    function Harness() {
      const [meta, setMeta] = useState(JSON.stringify({ [OWNED_PROJECT_COUNT_KEY]: 2 }));
      return (
        <div>
          <ParticipantList
            participants={[
              {
                identity: "meta-peer",
                role: "coder",
                joinedAt: 1_700_000_000_000,
                metadata: meta,
                codexOAuth: null,
              },
            ]}
            roomStatus="connected"
            connectionError={null}
          />
          <button
            type="button"
            data-testid="acceptance-bump-owned-project-count"
            onClick={() => setMeta(JSON.stringify({ [OWNED_PROJECT_COUNT_KEY]: 5 }))}
          >
            bump
          </button>
        </div>
      );
    }
    cy.mount(<Harness />);
    participantListPage.ownedProjectCount("meta-peer").should("have.text", "2");

    // When — trigger metadata update
    byTestId("acceptance-bump-owned-project-count").click();

    // Then
    participantListPage.ownedProjectCount("meta-peer").should("have.text", "5");
  });
});
