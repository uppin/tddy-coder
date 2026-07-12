/**
 * Behaviour spec: `SessionMainPane` must render the real Ghostty terminal for
 * `connected-livekit` sessions, using the same underlying terminal component
 * (`GhosttyTerminalLiveKit` → `GhosttyTerminal`) already used for Claude CLI's
 * LiveKit-routed sessions in `ConnectionScreen.tsx`.
 *
 * Today `connected-livekit` renders only a static placeholder ("Terminal
 * connected to {room}") — this is the only attachment path tddy-coder recipe
 * sessions (e.g. `plan-pr-stack`) ever reach, since `connect_session` always
 * returns a LiveKit room for any session type other than `claude-cli` /
 * `workspace`. Every test below fails today: `SessionMainPane` has no
 * `tokenClient` prop, and the placeholder renders instead of a terminal.
 *
 * Changeset: unify tddy-coder recipe-session terminals onto the same LiveKit
 * terminal component already used for Claude CLI.
 */

import React, { useMemo } from "react";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import { TokenService, GenerateTokenRequestSchema, GenerateTokenResponseSchema } from "../../src/gen/token_pb";
import { SessionMainPane } from "../../src/components/sessions/SessionMainPane";
import type { SessionAttachmentState } from "../../src/components/sessions/useSessionAttachment";
import { decodeProtoRequestBody, toArrayBuffer } from "../support/rpc/protoRpc";
import { byTestId, TEST_IDS } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const FAKE_SESSION = {
  sessionId: "livekit-terminal-test-aaaa-0000-0000-000000000001",
  createdAt: "2026-06-30T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/my-recipe-session",
  pid: 43001,
  isActive: true,
  projectId: "proj-livekit-terminal-1",
  daemonInstanceId: "",
  workflowGoal: "plan-pr-stack recipe session",
  pendingElicitation: false,
};

const LIVEKIT_ATTACHMENT: SessionAttachmentState = {
  status: "connected-livekit",
  sessionId: FAKE_SESSION.sessionId,
  livekitRoom: "tddy-lobby",
  livekitUrl: "ws://localhost:9999",
  livekitServerIdentity: "daemon-dev-livekit-terminal-test-0001",
  identity: "browser-livekit-terminal-test-aaaa-0000-0000-000000000001-1719999999999",
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

function LiveKitMainPaneHarness() {
  const transport = useMemo(
    () => createConnectTransport({ baseUrl: `${window.location.origin}/rpc`, useBinaryFormat: true }),
    [],
  );
  const tokenClient = useMemo(() => createClient(TokenService, transport), [transport]);

  return (
    <SessionMainPane
      selectedSession={FAKE_SESSION as any}
      attachment={LIVEKIT_ATTACHMENT}
      inspectorState="closed"
      onToggleInspector={cy.stub()}
      onInspectorClose={cy.stub()}
      onInspectorExpand={cy.stub()}
      onInspectorRestore={cy.stub()}
      onResume={cy.stub()}
      onDelete={cy.stub()}
      onTerminate={cy.stub()}
      tokenClient={tokenClient}
      runtimes={[
        {
          sessionId: FAKE_SESSION.sessionId,
          attached: true,
          status: "connected-livekit",
          livekitUrl: LIVEKIT_ATTACHMENT.livekitUrl,
          livekitRoom: LIVEKIT_ATTACHMENT.livekitRoom,
          livekitServerIdentity: LIVEKIT_ATTACHMENT.livekitServerIdentity,
          identity: (LIVEKIT_ATTACHMENT as { identity: string }).identity,
          bytesIn: 0,
          bytesOut: 0,
          lastDataReceivedAt: null,
        },
      ]}
      focusedRuntimeId={FAKE_SESSION.sessionId}
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

describe("SessionMainPane — LiveKit-routed sessions render a real terminal", () => {
  it("renders the Ghostty terminal for a connected-livekit session when a tokenClient is supplied", () => {
    // Given
    interceptGenerateToken();

    // When
    cy.mount(<LiveKitMainPaneHarness />);

    // Then
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");
  });

  it("does not render the static 'Terminal connected to' placeholder once a terminal is wired", () => {
    // Given
    interceptGenerateToken();

    // When
    cy.mount(<LiveKitMainPaneHarness />);
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");

    // Then
    byTestId(TEST_IDS.sessionsDetailPane).should("not.contain.text", "Terminal connected to");
  });

  it("requests a browser LiveKit token scoped to the session's room and identity", () => {
    // Given
    interceptGenerateToken();

    // When
    cy.mount(<LiveKitMainPaneHarness />);

    // Then
    cy.wait("@generateToken").then((interception) => {
      const req = fromBinary(GenerateTokenRequestSchema, decodeProtoRequestBody(interception.request.body));
      expect(req.room).to.equal(LIVEKIT_ATTACHMENT.livekitRoom);
      expect(req.identity).to.equal((LIVEKIT_ATTACHMENT as { identity: string }).identity);
    });
  });

  it("does not show a visible 'connecting'/'connected' status strip above the terminal", () => {
    // Given
    interceptGenerateToken();

    // When
    cy.mount(<LiveKitMainPaneHarness />);
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");

    // Then — the raw status strip stays in the DOM (for tooling) but must not be visible to the user
    byTestId(TEST_IDS.livekitStatus).should("exist").and("not.be.visible");
  });
});
