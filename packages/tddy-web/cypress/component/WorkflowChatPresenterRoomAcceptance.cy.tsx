/**
 * Acceptance test: the full-screen Workflow Chat Screen must open its own dedicated LiveKit room
 * connection for the session's remote Presenter, derived from the session's own
 * `connectSession`/`resumeSession` attachment — the same room/url the terminal independently
 * connects to. If it sat on `SessionMainPane`'s always-null VNC-purpose `room` prop, the chat could
 * never send or receive anything (the exact bug that was fixed for the pr-stack chat).
 *
 * PRD: docs/ft/web/session-drawer.md § Per-Workflow Session Views.
 */

import React from "react";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import { GenerateTokenRequestSchema, GenerateTokenResponseSchema } from "../../src/gen/token_pb";
import { SessionMainPane } from "../../src/components/sessions/SessionMainPane";
import type { SessionAttachmentState } from "../../src/components/sessions/useSessionAttachment";
import { decodeProtoRequestBody, toArrayBuffer } from "../support/rpc/protoRpc";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const TDD_SESSION = {
  sessionId: "tdd-presenter-room-aaaa-0000-0000-000000000001",
  createdAt: "2026-07-01T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/workflow-chat-project",
  pid: 0,
  isActive: true,
  projectId: "proj-workflow-chat-presenter-room",
  daemonInstanceId: "",
  workflowGoal: "",
  pendingElicitation: false,
  orchestratorSessionId: "",
  recipe: "tdd",
  sessionType: "tool",
  stackPlanJson: "",
};

const LIVEKIT_ATTACHMENT: SessionAttachmentState = {
  status: "connected-livekit",
  sessionId: TDD_SESSION.sessionId,
  livekitRoom: "daemon-tdd-presenter-room-0001",
  livekitUrl: "wss://livekit.internal:7880",
  livekitServerIdentity: "daemon-tdd-presenter-room-0001",
  identity: "browser-tdd-presenter-room-aaaa-0000-0000-000000000001-1719999999999",
};

const GENERATE_TOKEN_OK = toArrayBuffer(
  toBinary(
    GenerateTokenResponseSchema,
    create(GenerateTokenResponseSchema, { token: "lk-session-token", ttlSeconds: BigInt(600) }),
  ),
);

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

function WorkflowChatMainPaneHarness() {
  return (
    <SessionMainPane
      selectedSession={TDD_SESSION as any}
      attachment={LIVEKIT_ATTACHMENT}
      inspectorState="closed"
      onToggleInspector={cy.stub()}
      onInspectorClose={cy.stub()}
      onInspectorExpand={cy.stub()}
      onInspectorRestore={cy.stub()}
      onResume={cy.stub()}
      onDelete={cy.stub()}
      onTerminate={cy.stub()}
    />
  );
}

function interceptGenerateToken() {
  cy.intercept("POST", "**/rpc/token.TokenService/GenerateToken", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: GENERATE_TOKEN_OK });
  }).as("generateToken");
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("Workflow Chat Screen — opens its own presenter LiveKit room from the session's attachment", () => {
  it("requests a browser LiveKit token scoped to the session's attached room, not the always-null VNC room", () => {
    // Given — a tool workflow session already attached over LiveKit (connectSession has resolved)
    interceptGenerateToken();

    // When
    cy.mount(<WorkflowChatMainPaneHarness />);

    // Then — the presenter connection targets the session's own attached room, proving
    // WorkflowChatScreen opened its own LiveKit connection instead of sitting on the always-null
    // `room` prop forever
    cy.wait("@generateToken").then((interception) => {
      const req = fromBinary(GenerateTokenRequestSchema, decodeProtoRequestBody(interception.request.body));
      expect(req.room).to.equal(LIVEKIT_ATTACHMENT.livekitRoom);
    });
  });
});
