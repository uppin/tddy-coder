/**
 * Behaviour spec: `useSessionAttachment` must mint a stable browser LiveKit
 * `identity` alongside the `connected-livekit` attachment state.
 *
 * Changeset: unify tddy-coder recipe-session terminals onto the same LiveKit
 * terminal component already used for Claude CLI (`GhosttyTerminalLiveKit`).
 * That component (and the shared `TokenService.generateToken` call it needs)
 * requires a per-browser-tab `identity` string scoped to the session's LiveKit
 * room — mirrored from the existing `ConnectionScreen.tsx` pattern
 * (`browser-${sessionId}-${Date.now()}`).
 *
 * `SessionAttachmentState`'s `connected-livekit` variant does not carry this
 * field today, so every test below fails to type-check (excess/missing
 * property) until the field and its generation are added to
 * `useSessionAttachment.ts`.
 */

import React, { useMemo } from "react";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import { create, toBinary } from "@bufbuild/protobuf";
import { ConnectionService, ConnectSessionResponseSchema } from "../../src/gen/connection_pb";
import { useSessionAttachment } from "../../src/components/sessions/useSessionAttachment";
import { toArrayBuffer } from "../support/rpc/protoRpc";
import { byTestId, TEST_IDS } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const SESSION_ID = "attach-identity-test-session-0001";
const SESSION_TOKEN = "test-session-token-attach-identity";

const LIVEKIT_CONNECT_OK = toArrayBuffer(
  toBinary(
    ConnectSessionResponseSchema,
    create(ConnectSessionResponseSchema, {
      livekitRoom: "room-attach-identity-0001",
      livekitUrl: "wss://livekit.example.internal",
      livekitServerIdentity: "daemon-dev-attach-identity-0001",
    }),
  ),
);

const GRPC_CONNECT_OK = toArrayBuffer(
  toBinary(
    ConnectSessionResponseSchema,
    create(ConnectSessionResponseSchema, {
      livekitRoom: "",
      livekitUrl: "",
      livekitServerIdentity: "",
    }),
  ),
);

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

const CONNECT_BTN = "attach-identity-connect-btn";
const STATUS_EL = "attach-identity-status";
const IDENTITY_EL = "attach-identity-value";

function AttachmentHarness() {
  const transport = useMemo(
    () => createConnectTransport({ baseUrl: `${window.location.origin}/rpc`, useBinaryFormat: true }),
    [],
  );
  const client = useMemo(() => createClient(ConnectionService, transport), [transport]);
  const { state, connectSession } = useSessionAttachment();

  return (
    <div>
      <button
        type="button"
        data-testid={CONNECT_BTN}
        onClick={() => void connectSession(SESSION_ID, SESSION_TOKEN, client)}
      >
        connect
      </button>
      <span data-testid={STATUS_EL}>{state.status}</span>
      <span data-testid={IDENTITY_EL}>
        {state.status === "connected-livekit" ? state.identity : ""}
      </span>
    </div>
  );
}

function interceptConnectSession(body: ArrayBuffer) {
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ConnectSession", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body });
  }).as("connectSession");
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("useSessionAttachment — LiveKit browser identity", () => {
  it("mints a browser identity scoped to the session when the response carries a LiveKit room", () => {
    // Given
    interceptConnectSession(LIVEKIT_CONNECT_OK);
    cy.mount(<AttachmentHarness />);

    // When
    byTestId(CONNECT_BTN).click();
    cy.wait("@connectSession");

    // Then
    byTestId(STATUS_EL).should("have.text", "connected-livekit");
    byTestId(IDENTITY_EL)
      .invoke("text")
      .should("match", new RegExp(`^browser-${SESSION_ID}-\\d+$`));
  });

  it("does not attach an identity to the connected-grpc state (claude-cli / workspace sessions)", () => {
    // Given
    interceptConnectSession(GRPC_CONNECT_OK);
    cy.mount(<AttachmentHarness />);

    // When
    byTestId(CONNECT_BTN).click();
    cy.wait("@connectSession");

    // Then
    byTestId(STATUS_EL).should("have.text", "connected-grpc");
    byTestId(IDENTITY_EL).should("have.text", "");
  });
});
