/**
 * Behaviour spec: `SessionMainPane` must render the real Ghostty terminal for a session whose
 * connection carries tracks, using the same underlying terminal component
 * (`GhosttyTerminalLiveKit` → `GhosttyTerminal`) already used for Claude CLI's
 * LiveKit-routed sessions in `ConnectionScreen.tsx`.
 *
 * Such a session used to render only a static placeholder ("Terminal
 * connected to {room}") — this is the only attachment path tddy-coder recipe
 * sessions (e.g. `plan-pr-stack`) ever reach, since `connect_session` always
 * returns a LiveKit room for any session type other than `claude-cli` /
 * `workspace`.
 *
 * Changeset: unify tddy-coder recipe-session terminals onto the same LiveKit
 * terminal component already used for Claude CLI.
 */

import React, { useMemo } from "react";
import { createClient, type Client } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { TokenService, GenerateTokenRequestSchema, GenerateTokenResponseSchema } from "../../src/gen/token_pb";
import { SessionMainPane } from "../../src/components/sessions/SessionMainPane";
import type { SessionAttachmentState } from "../../src/components/sessions/useSessionAttachment";
import { aSessionConnection } from "../support/rpc/sessionConnections";
import { useHttpClient } from "../../src/rpc/transportProvider";
import { decodeProtoRequestBody, toArrayBuffer } from "../support/rpc/protoRpc";
import { withSessionTokenGate } from "../support/rpc/withSessionTokenGate";
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

/** The session's own RPC route. Nothing here asserts on it — these specs are about the terminal the
 *  connection's `media` capability selects — so an empty backend serves it. */
const ATTACHED_OVER_A_ROOM = aSessionConnection(FAKE_SESSION.sessionId)
  .carriedByRoom("tddy-lobby", {
    url: "ws://localhost:9999",
    serverIdentity: "daemon-dev-livekit-terminal-test-0001",
  })
  .servingOver(anInMemoryRpcBackend().transport());
const LIVEKIT_CONNECTION = ATTACHED_OVER_A_ROOM.build();
const LIVEKIT_ATTACHMENT: SessionAttachmentState = {
  status: "connected",
  connection: LIVEKIT_CONNECTION,
};
const LIVEKIT_HINT = ATTACHED_OVER_A_ROOM.buildHint();

/** The signed-in operator's daemon access token — what the daemon's mint refuses to act without. */
const SESSION_TOKEN = "an-operator-access-token";

const GENERATE_TOKEN_OK = toArrayBuffer(
  toBinary(
    GenerateTokenResponseSchema,
    create(GenerateTokenResponseSchema, { token: "lk-session-token", ttlSeconds: BigInt(600) }),
  ),
);

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

function LiveKitMainPaneHarness({ tokenClient: injected }: { tokenClient?: Client<typeof TokenService> } = {}) {
  const transport = useMemo(
    () => createConnectTransport({ baseUrl: `${window.location.origin}/rpc`, useBinaryFormat: true }),
    [],
  );
  const ownClient = useMemo(() => createClient(TokenService, transport), [transport]);
  const tokenClient = injected ?? ownClient;

  return (
    <SessionMainPane
      selectedSession={FAKE_SESSION as any}
      attachment={LIVEKIT_ATTACHMENT}
      attachmentHint={LIVEKIT_HINT}
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
          connection: LIVEKIT_CONNECTION,
          hint: LIVEKIT_HINT,
          bytesIn: 0,
          bytesOut: 0,
          lastDataReceivedAt: null,
        },
      ]}
      focusedRuntimeId={FAKE_SESSION.sessionId}
    />
  );
}

/** The same pane whose token client comes from the app transport, so requests pass the auth gate. */
function GatedLiveKitMainPaneHarness() {
  const tokenClient = useHttpClient(TokenService);
  return <LiveKitMainPaneHarness tokenClient={tokenClient} />;
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
  it("renders the Ghostty terminal for a room-carried session when a tokenClient is supplied", () => {
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
      expect(req.room).to.equal(LIVEKIT_HINT.room);
      // The browser's own participant identity is minted per join, so only its shape can be pinned:
      // it must name this session, must not collide with the session process's own identity, and
      // carries a random tail so two joins landing in the same millisecond are still two
      // participants — see `SessionTerminalIdentity.cy.tsx` for that half.
      expect(req.identity).to.match(new RegExp(`^browser-${FAKE_SESSION.sessionId}-\\d+-[a-z0-9]+$`));
    });
  });

  it("authenticates the token request with the signed-in operator's session token", () => {
    // Given a signed-in operator — the daemon's mint refuses an anonymous caller
    interceptGenerateToken();

    // When
    cy.mount(withSessionTokenGate(SESSION_TOKEN, <GatedLiveKitMainPaneHarness />));

    // Then
    cy.wait("@generateToken").then((interception) => {
      const req = fromBinary(GenerateTokenRequestSchema, decodeProtoRequestBody(interception.request.body));
      expect(req.sessionToken).to.equal(SESSION_TOKEN);
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
