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
import { ConnectionState, Room } from "livekit-client";
import { ConnectSessionResponseSchema } from "../../src/gen/connection_pb";
import { TokenService, GenerateTokenResponseSchema } from "../../src/gen/token_pb";
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

/** The browser token the room-backed join is issued. Nothing asserts on it; it exists so the join
 *  has a determinate outcome rather than a swallowed failure. */
const GENERATE_TOKEN_OK = toArrayBuffer(
  toBinary(
    GenerateTokenResponseSchema,
    create(GenerateTokenResponseSchema, { token: "lk-browser-token", ttlSeconds: BigInt(600) }),
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
 * A session room that records its join instead of reaching a media server.
 *
 * The room-backed reply starts a real join, and a real `Room` here would reach for
 * `wss://livekit.example.internal`, fail somewhere in the SDK's own retry schedule, and leave the
 * spec's outcome depending on how long that took. What these specs assert is the routing the reply
 * produced, so the join is made to settle instead of being left to fail on its own time.
 */
function aRoomThatJoinsAtOnce(): Room {
  const room = {
    state: ConnectionState.Disconnected,
    remoteParticipants: new Map<string, { identity: string }>(),
    connect: async () => {
      room.state = ConnectionState.Connected;
    },
    disconnect: async () => {
      room.state = ConnectionState.Disconnected;
    },
  };
  return room as unknown as Room;
}

/**
 * The host connection the attach runs against — the real LiveKit provider, bound to this page's own
 * HTTP transport so `ConnectSession` reaches the intercept, and given the resources a room-backed
 * session's join needs.
 */
function aHostConnection(): HostConnection {
  const transport = createConnectTransport({
    baseUrl: `${window.location.origin}/rpc`,
    useBinaryFormat: true,
  });
  const provider = new LiveKitConnectionProvider(new Room(), () => transport, {
    tokens: createClient(TokenService, transport),
    newRoom: aRoomThatJoinsAtOnce,
  });
  const host = provider.connectHost(A_HOST);
  if (!host) throw new Error("the LiveKit provider must claim a host once it has a room");
  return host;
}

function interceptGenerateToken() {
  cy.intercept("POST", "**/rpc/token.TokenService/GenerateToken", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: GENERATE_TOKEN_OK,
    });
  }).as("generateToken");
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
    interceptGenerateToken();
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
