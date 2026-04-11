import React, { useState } from "react";
import { ParticipantList } from "../../src/components/ParticipantList";
import type { RoomParticipant } from "../../src/hooks/useRoomParticipants";

function mountParticipantList(props: React.ComponentProps<typeof ParticipantList>) {
  cy.mount(<ParticipantList {...props} />);
}

describe("ParticipantList", () => {
  it("shows connecting state", () => {
    cy.mount(
      <ParticipantList participants={[]} roomStatus="connecting" connectionError={null} />
    );
    cy.get("[data-testid='participant-list']").should("have.attr", "data-room-status", "connecting");
    cy.contains("Connecting to presence room");
  });

  it("shows error state", () => {
    cy.mount(
      <ParticipantList participants={[]} roomStatus="error" connectionError="token failed" />
    );
    cy.get("[data-testid='participant-list']").should("have.attr", "data-room-status", "error");
    cy.get("[data-testid='participant-list-error']").should("contain.text", "token failed");
  });

  it("shows empty state when connected with no rows", () => {
    cy.mount(
      <ParticipantList participants={[]} roomStatus="connected" connectionError={null} />
    );
    cy.get("[data-testid='participant-list-empty']").should("exist");
  });

  it("renders participant rows with role badges", () => {
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
    cy.mount(
      <ParticipantList participants={participants} roomStatus="connected" connectionError={null} />
    );
    cy.get("[data-testid='participant-entry-web-testuser']").should("contain.text", "web-testuser");
    cy.get("[data-testid='participant-role-web-testuser']").should("contain.text", "browser");
    cy.get("[data-testid='participant-role-server-abc']").should("contain.text", "coder");
    cy.get("[data-testid='participant-metadata-server-abc']").should("contain.text", '{"k":"v"}');
  });

  it("labels common-room daemon advertisement as daemon", () => {
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
    cy.mount(
      <ParticipantList participants={participants} roomStatus="connected" connectionError={null} />,
    );
    cy.get("[data-testid='participant-role-my-host']").should("contain.text", "daemon");
  });

  it("shows Codex OAuth sign-in link when participant metadata requests it", () => {
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
    cy.mount(
      <ParticipantList participants={participants} roomStatus="connected" connectionError={null} />
    );
    cy.get("[data-testid='participant-codex-oauth-daemon-session']")
      .find("a")
      .should("have.attr", "href", "https://auth.example.com/oauth/authorize")
      .should("have.attr", "target", "_blank");
    cy.get("[data-testid='participant-metadata-daemon-session']")
      .find("a")
      .should("have.attr", "href", "https://auth.example.com/oauth/authorize")
      .should("contain.text", "open sign-in");
  });

  it("ParticipantList hides video affordance when participant has no camera track", () => {
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
    mountParticipantList({
      participants: [withCam, noCam],
      roomStatus: "connected",
      connectionError: null,
      participantHasCameraVideo: { "with-cam-user": true, "no-cam-user": false },
    });
    cy.get("[data-testid='participant-video-trigger-with-cam-user']").should("be.visible");
    cy.get("[data-testid='participant-video-trigger-no-cam-user']").should("not.exist");
  });

  it("ParticipantList shows video affordance when participant exposes camera video", () => {
    const participants: RoomParticipant[] = [
      {
        identity: "camera-peer",
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
      participantHasCameraVideo: { "camera-peer": true },
    });
    cy.get("[data-testid='participant-video-trigger-camera-peer']").should("be.visible");
    cy.get("[data-testid='participant-video-trigger-camera-peer']")
      .should("have.attr", "aria-label")
      .and("match", /camera-peer/i);
  });

  it("Activating video affordance opens dialog with video preview region", () => {
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
    cy.get("[data-testid='participant-video-trigger-preview-peer']").should("be.visible").click();
    cy.get("[data-testid='participant-video-dialog']").should("be.visible");
    cy.get("[data-testid='participant-video-dialog']").should("have.attr", "role", "dialog");
    cy.get("[data-testid='participant-video-preview']").should("be.visible");
  });

  it("Closing dialog removes dialog and preview from the document", () => {
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
    cy.get("[data-testid='participant-video-trigger-cleanup-peer']").should("be.visible").click();
    cy.get("[data-testid='participant-video-dialog']").should("be.visible");
    cy.get("[data-testid='participant-video-dialog']").within(() => {
      cy.get("[data-testid='participant-video-dialog-close']").click();
    });
    cy.get("[data-testid='participant-video-dialog']").should("not.exist");
    cy.get("[data-testid='participant-video-preview']").should("not.exist");
  });

  /** Canonical LiveKit metadata key (see `OWNED_PROJECT_COUNT_METADATA_KEY` in tddy-livekit). */
  const OWNED_PROJECT_COUNT_KEY = "owned_project_count";

  it("participant_list_renders_project_count", () => {
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
    cy.mount(
      <ParticipantList participants={participants} roomStatus="connected" connectionError={null} />,
    );
    cy.get("[data-testid='participant-owned-project-count-server-agent']").should("have.text", String(count));
  });

  it("participant_list_updates_when_metadata_changes", () => {
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
    cy.get("[data-testid='participant-owned-project-count-meta-peer']").should("have.text", "2");
    cy.get("[data-testid='acceptance-bump-owned-project-count']").click();
    cy.get("[data-testid='participant-owned-project-count-meta-peer']").should("have.text", "5");
  });
});
