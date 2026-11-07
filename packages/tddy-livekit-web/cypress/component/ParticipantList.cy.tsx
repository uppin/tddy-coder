import React, { useState } from "react";
import { ParticipantList } from "../../src";
import type { RoomParticipant } from "../../src";
import { participantListPage } from "../support/pages/participantListPage";
import { byTestId } from "../support/testIds";
import { mountRosterWithCameras } from "../support/cameraRoster";

/** Canonical LiveKit metadata key (see `OWNED_PROJECT_COUNT_METADATA_KEY` in tddy-livekit). */
const OWNED_PROJECT_COUNT_KEY = "owned_project_count";

function mountParticipantList(props: React.ComponentProps<typeof ParticipantList>, withCamera: readonly string[] = []) {
  mountRosterWithCameras(props, withCamera);
}

/**
 * The wire a roster is read over, as a LiveKit common room advertises it.
 *
 * Every scenario here has to say which wire it is on, because the panel reads two capabilities off
 * it: `presence` decides whether there is a roster to render at all, and `media` decides the camera
 * column. The wires that answer "no" to either are covered by their own specs —
 * `ParticipantVideoCapabilityAcceptance.cy.tsx` for tracks, `PresenceCapabilityGatingAcceptance.cy.tsx`
 * for the roster.
 */
const A_WIRE_THAT_CARRIES_TRACKS = { capabilities: new Set(["rpc", "media", "presence"] as const) };

describe("ParticipantList", () => {
  it("shows connecting state", () => {
    // Given / When
    cy.mount(
      <ParticipantList
        participants={[]}
        roomStatus="connecting"
        connectionError={null}
        connection={A_WIRE_THAT_CARRIES_TRACKS}
      />
    );

    // Then
    participantListPage.list().should("have.attr", "data-room-status", "connecting");
    cy.contains("Connecting to presence room");
  });

  it("shows error state", () => {
    // Given / When
    cy.mount(
      <ParticipantList
        participants={[]}
        roomStatus="error"
        connectionError="token failed"
        connection={A_WIRE_THAT_CARRIES_TRACKS}
      />
    );

    // Then
    participantListPage.list().should("have.attr", "data-room-status", "error");
    participantListPage.error().should("contain.text", "token failed");
  });

  it("shows empty state when connected with no participants", () => {
    // Given / When
    cy.mount(
      <ParticipantList
        participants={[]}
        roomStatus="connected"
        connectionError={null}
        connection={A_WIRE_THAT_CARRIES_TRACKS}
      />
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
      <ParticipantList
        participants={participants}
        roomStatus="connected"
        connectionError={null}
        connection={A_WIRE_THAT_CARRIES_TRACKS}
      />
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
      <ParticipantList
        participants={participants}
        roomStatus="connected"
        connectionError={null}
        connection={A_WIRE_THAT_CARRIES_TRACKS}
      />,
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
      <ParticipantList
        participants={participants}
        roomStatus="connected"
        connectionError={null}
        connection={A_WIRE_THAT_CARRIES_TRACKS}
      />
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
      connection: A_WIRE_THAT_CARRIES_TRACKS,
    }, ["with-cam-user"]);

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
      connection: A_WIRE_THAT_CARRIES_TRACKS,
    }, ["camera-peer"]);

    // Then
    participantListPage.videoTrigger("camera-peer").should("be.visible");
    participantListPage.videoTrigger("camera-peer")
      .should("have.attr", "aria-label")
      .and("match", /camera-peer/i);
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
      <ParticipantList
        participants={participants}
        roomStatus="connected"
        connectionError={null}
        connection={A_WIRE_THAT_CARRIES_TRACKS}
      />,
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
            connection={A_WIRE_THAT_CARRIES_TRACKS}
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
