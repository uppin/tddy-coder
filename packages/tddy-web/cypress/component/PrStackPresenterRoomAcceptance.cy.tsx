/**
 * Acceptance test: the PR-Stack Chat Screen must open its own dedicated LiveKit room
 * connection for the session's remote Presenter, derived from the session's own
 * `connectSession`/`resumeSession` attachment — the same room/url the terminal
 * (`SessionLiveKitTerminal`) independently connects to. Today it is handed
 * `SessionMainPane`'s VNC-purpose `room` prop instead, which is always `null`, so the
 * chat can never actually send or receive anything.
 *
 * PRD: docs/ft/web/session-drawer.md § Per-Workflow Session Views (chat window).
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

const PR_STACK_SESSION = {
  sessionId: "pr-stack-presenter-room-aaaa-0000-0000-000000000001",
  createdAt: "2026-07-01T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/pr-stack-project",
  pid: 0,
  isActive: true,
  projectId: "proj-pr-stack-presenter-room",
  daemonInstanceId: "",
  workflowGoal: "",
  pendingElicitation: false,
  orchestratorSessionId: "",
  recipe: "pr-stack",
  stackPlanJson: "",
};

const LIVEKIT_ATTACHMENT: SessionAttachmentState = {
  status: "connected-livekit",
  sessionId: PR_STACK_SESSION.sessionId,
  livekitRoom: "daemon-pr-stack-presenter-room-0001",
  livekitUrl: "wss://livekit.internal:7880",
  livekitServerIdentity: "daemon-pr-stack-presenter-room-0001",
  identity: "browser-pr-stack-presenter-room-aaaa-0000-0000-000000000001-1719999999999",
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

function PrStackMainPaneHarness() {
  return (
    <SessionMainPane
      selectedSession={PR_STACK_SESSION as any}
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

describe("PR-Stack Chat Screen — opens its own presenter LiveKit room from the session's attachment", () => {
  it("requests a browser LiveKit token scoped to the session's attached room, not the always-null VNC room", () => {
    // Given — a pr-stack session already attached over LiveKit (connectSession has resolved)
    interceptGenerateToken();

    // When
    cy.mount(<PrStackMainPaneHarness />);

    // Then — the presenter connection targets the session's own attached room, proving
    // PrStackScreen opened its own LiveKit connection instead of sitting on the always-null
    // `room` prop forever
    cy.wait("@generateToken").then((interception) => {
      const req = fromBinary(GenerateTokenRequestSchema, decodeProtoRequestBody(interception.request.body));
      expect(req.room).to.equal(LIVEKIT_ATTACHMENT.livekitRoom);
    });
  });
});
