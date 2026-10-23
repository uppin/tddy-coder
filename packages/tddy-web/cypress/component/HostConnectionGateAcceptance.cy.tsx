/**
 * Acceptance tests: a host-scoped screen is blocked, and says why, when no viable RPC client reaches
 * the selected host.
 *
 * With the LiveKit server down, the common room's join failed, no connection reached the host, and
 * the Projects screen rendered a Create button that did nothing. The layout now covers the body of
 * every host-scoped screen with a gate naming the host and the reason. The header stays usable so
 * the operator can pick another host or leave. The decision itself is unit-tested in
 * `src/components/shell/hostGate.test.ts`; these pin that screens render it.
 */

import React from "react";
import { Room } from "livekit-client";
import { daemonRpcIdentity } from "../../src/lib/participantRole";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aJoinedCommonRoom } from "../support/rpc/withSelectedDaemon";
import {
  aProjectsBackend,
  PROJECTS_LOCAL_HOST as LOCAL_HOST,
  projectsAppPage,
} from "../support/rpc/projectsAppPageFixtures";
import { hostConnectionGatePage } from "../support/pages/hostConnectionGatePage";
import { projectsScreenPage } from "../support/pages/projectsScreenPage";

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

it("blocks the screen and names the host when no connection reaches it", () => {
  // Given a selected host and no wire to it
  // When
  cy.mount(projectsAppPage(null));

  // Then
  hostConnectionGatePage.gate().should("be.visible");
  hostConnectionGatePage.title().should("contain.text", "Not connected to workstation-1");
  hostConnectionGatePage.reason().should("contain.text", "LiveKit is not configured");
});

it("makes the screen behind the gate unusable", () => {
  // Given
  cy.mount(projectsAppPage(null));

  // Then the body is inert, so its controls cannot be reached by pointer or keyboard
  hostConnectionGatePage.gatedBody().should("have.attr", "inert");
  hostConnectionGatePage.gatedBody().should("have.attr", "aria-hidden", "true");
});

it("keeps the header usable while the body is blocked", () => {
  // Given
  cy.mount(projectsAppPage(null));

  // Then
  hostConnectionGatePage.gate().should("be.visible");
  hostConnectionGatePage.navigationMenuButton().should("be.visible").click();
});

it("does not block when a connection reaches the host over a joined common room", () => {
  // Given
  const room = aJoinedCommonRoom([daemonRpcIdentity(LOCAL_HOST)]);

  // When
  mountWithRpc(projectsAppPage(room), aProjectsBackend());

  // Then
  projectsScreenPage.screen().should("be.visible");
  hostConnectionGatePage.gate().should("not.exist");
});

it("does not block while a connection exists and its room is still settling", () => {
  // Given a connection over a room that has not finished joining — single calls report inline
  // When
  mountWithRpc(projectsAppPage(new Room()), aProjectsBackend());

  // Then
  projectsScreenPage.screen().should("be.visible");
  hostConnectionGatePage.gate().should("not.exist");
});
