/**
 * Behaviour spec: `useSessionAttachment` reads the daemon's attach reply into one connected state,
 * whatever the reply said about how to reach the session.
 *
 * The reply carries LiveKit fields, and the hook used to branch on them — a room meant
 * `connected-livekit` (carrying four LiveKit fields and a minted browser identity), no room meant a
 * second, degraded `connected-grpc`. Every consumer then had to know which of the two it was
 * looking at. Now the reply becomes a `SessionAttachmentHint`, the host opens a `SessionConnection`
 * over it, and what the session can do is the connection's `capabilities` rather than its status.
 *
 * Changeset: `docs/dev/1-WIP/2026-09-05-optional-livekit-session-connection.md`.
 */

import React, { useMemo } from "react";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import { create, toBinary } from "@bufbuild/protobuf";
import { Room } from "livekit-client";
import { ConnectSessionResponseSchema } from "../../src/gen/connection_pb";
import { TokenService } from "../../src/gen/token_pb";
import { useSessionAttachment } from "../../src/components/sessions/useSessionAttachment";
import { LiveKitConnectionProvider } from "../../src/rpc/connections/liveKit";
import type { HostConnection } from "../../src/rpc/connections/types";
import { toArrayBuffer } from "../support/rpc/protoRpc";
import { byTestId } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const A_HOST = "instance-attach-test";
const SESSION_ID = "attach-routing-test-session-0001";
const SESSION_TOKEN = "test-session-token-attach-routing";
const A_ROOM = "room-attach-routing-0001";

/** What the daemon replies for a session published into a room of its own. */
const ROOM_BACKED_CONNECT_OK = toArrayBuffer(
  toBinary(
    ConnectSessionResponseSchema,
    create(ConnectSessionResponseSchema, {
      livekitRoom: A_ROOM,
      livekitUrl: "wss://livekit.example.internal",
      livekitServerIdentity: `daemon-dev-${SESSION_ID}`,
    }),
  ),
);

/** What it replies for a session it serves itself — `cli_session_manager.rs`'s PTY handle. */
const HOST_SERVED_CONNECT_OK = toArrayBuffer(
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

const CONNECT_BTN = "attach-routing-connect-btn";
const STATUS_EL = "attach-routing-status";
const CAPABILITIES_EL = "attach-routing-capabilities";
const ROOM_EL = "attach-routing-room";

/**
 * The host connection the attach runs against — the real LiveKit provider, bound to this page's own
 * HTTP transport so `ConnectSession` reaches the intercept, and given the resources a room-backed
 * session's join needs (no media server answers them here; what these specs assert is the routing
 * the reply produced, not the join it started).
 */
function aHostConnection(): HostConnection {
  const transport = createConnectTransport({
    baseUrl: `${window.location.origin}/rpc`,
    useBinaryFormat: true,
  });
  const provider = new LiveKitConnectionProvider(new Room(), () => transport, {
    tokens: createClient(TokenService, transport),
    newRoom: () => new Room(),
  });
  const host = provider.connectHost(A_HOST);
  if (!host) throw new Error("the LiveKit provider must claim a host once it has a room");
  return host;
}

function AttachmentHarness() {
  const host = useMemo(aHostConnection, []);
  const { state, hint, connectSession } = useSessionAttachment();

  return (
    <div>
      <button
        type="button"
        data-testid={CONNECT_BTN}
        onClick={() => void connectSession(SESSION_ID, SESSION_TOKEN, host)}
      >
        connect
      </button>
      <span data-testid={STATUS_EL}>{state.status}</span>
      <span data-testid={CAPABILITIES_EL}>
        {state.status === "connected" ? [...state.connection.capabilities].sort().join(",") : ""}
      </span>
      <span data-testid={ROOM_EL}>{hint?.room ?? ""}</span>
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

describe("useSessionAttachment — one connected state, whatever the reply routed to", () => {
  it("opens a media-capable connection when the reply names a room", () => {
    // Given
    interceptConnectSession(ROOM_BACKED_CONNECT_OK);
    cy.mount(<AttachmentHarness />);

    // When
    byTestId(CONNECT_BTN).click();
    cy.wait("@connectSession");

    // Then the attachment is simply connected, and what the session can do is the connection's to
    // say — the question the two statuses were being used to answer
    byTestId(STATUS_EL).should("have.text", "connected");
    byTestId(CAPABILITIES_EL).should("have.text", "media,presence,rpc");
    byTestId(ROOM_EL).should("have.text", A_ROOM);
  });

  it("opens an rpc-only connection when the reply names no room", () => {
    // Given a session the host serves itself — today's `connected-grpc`, and what a desktop app
    // over IPC produces
    interceptConnectSession(HOST_SERVED_CONNECT_OK);
    cy.mount(<AttachmentHarness />);

    // When
    byTestId(CONNECT_BTN).click();
    cy.wait("@connectSession");

    // Then it reaches the same connected state — not a lesser one — advertising plain RPC, and
    // carries no room for anything to try to join
    byTestId(STATUS_EL).should("have.text", "connected");
    byTestId(CAPABILITIES_EL).should("have.text", "rpc");
    byTestId(ROOM_EL).should("have.text", "");
  });
});
