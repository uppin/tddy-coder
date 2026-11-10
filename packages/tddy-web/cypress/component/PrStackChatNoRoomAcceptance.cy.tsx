/**
 * Acceptance: the PR-Stack chat renders before any LiveKit room is attached.
 *
 * Stays in tddy-web: it deliberately runs through the app's real default LiveKit transport factory
 * (no in-memory override), to catch a factory that assumes a non-null room — a property of the
 * app's wiring that the package's port doubles cannot exercise.
 *
 * Moved out of `tddy-session-web`'s `PrStackChatAcceptance.cy.tsx` (#web-split 18/20).
 */

import React from "react";
import { PrStackChat } from "tddy-prstack-web";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";

const PR_STACK_SESSION = {
  sessionId: "pr-stack-session-3333-0000-0000-0000-000000000030",
  createdAt: "2026-07-01T09:00:00Z",
  status: "idle",
  repoPath: "/home/dev/pr-stack-project",
  pid: 0,
  isActive: false,
  projectId: "proj-pr-stack",
  daemonInstanceId: "",
  workflowGoal: "",
  pendingElicitation: false,
  orchestratorSessionId: "",
  recipe: "pr-stack",
  stackPlanJson: "",
};

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("renders the chat panel when no LiveKit room has been attached to the session yet", () => {
  // Given — a pr-stack session before any LiveKit room is connected: the real production
  // state today, since nothing yet threads a connected orchestrator Room through to this
  // screen (see the TODO in usePresenterChat.ts). No RpcTransportProvider override here —
  // this deliberately exercises the real default LiveKit transport factory, not the
  // in-memory fake the other tests in this file use, to catch a factory that assumes a
  // non-null room.

  // When
  cy.mount(<PrStackChat session={PR_STACK_SESSION as any} room={null} livekitServerIdentity="server" />);

  // Then
  prStackScreenPage.chat().should("exist");
});
