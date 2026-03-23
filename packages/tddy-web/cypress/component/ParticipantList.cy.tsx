import React from "react";
import { ParticipantList } from "../../src/components/ParticipantList";
import type { RoomParticipant } from "../../src/hooks/useRoomParticipants";

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
      },
      {
        identity: "server-abc",
        role: "server",
        joinedAt: 1_700_000_000_000,
        metadata: '{"k":"v"}',
      },
    ];
    cy.mount(
      <ParticipantList participants={participants} roomStatus="connected" connectionError={null} />
    );
    cy.get("[data-testid='participant-entry-web-testuser']").should("contain.text", "web-testuser");
    cy.get("[data-testid='participant-role-web-testuser']").should("contain.text", "browser");
    cy.get("[data-testid='participant-role-server-abc']").should("contain.text", "server");
    cy.get("[data-testid='participant-metadata-server-abc']").should("contain.text", '{"k":"v"}');
  });
});
