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
 * Technical: `packages/tddy-web/docs/session-connections.md`.
 */

import React, { useMemo } from "react";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ConnectSessionResponseSchema, SessionService, type ConnectSessionResponse } from "tddy-client-web/gen/session_pb";
import { useSessionAttachment } from "../../src/useSessionAttachment";
import type { HostConnection } from "tddy-client-web/connections/types";
import { aHostConnection } from "../support/rpc/hostConnections";
import { byTestId } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const A_HOST = "instance-attach-test";
const SESSION_ID = "attach-routing-test-session-0001";
const SESSION_TOKEN = "test-session-token-attach-routing";
const A_ROOM = "room-attach-routing-0001";

/** What the daemon replies for a session published into a room of its own. */
const ROOM_BACKED_CONNECT_OK = create(ConnectSessionResponseSchema, {
  livekitRoom: A_ROOM,
  livekitUrl: "wss://livekit.example.internal",
  livekitServerIdentity: `daemon-dev-${SESSION_ID}`,
});

/** What it replies for a session it serves itself — `cli_session_manager.rs`'s PTY handle. */
const HOST_SERVED_CONNECT_OK = create(ConnectSessionResponseSchema, {
  livekitRoom: "",
  livekitUrl: "",
  livekitServerIdentity: "",
});

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

const CONNECT_BTN = "attach-routing-connect-btn";
const STATUS_EL = "attach-routing-status";
const CAPABILITIES_EL = "attach-routing-capabilities";
const ROOM_EL = "attach-routing-room";

/**
 * The host connection the attach runs against: a double whose `ConnectSession` answers `reply`, and
 * whose `openSession` is the production host-served one — what a reply names (a room or none) is
 * read into capabilities exactly as in production, with no media server behind it.
 */
function aHostAnswering(reply: ConnectSessionResponse): HostConnection {
  const backend = anInMemoryRpcBackend().onUnary(SessionService.method.connectSession, () => reply);
  return aHostConnection(A_HOST).servingOver(backend.transport()).build();
}

function AttachmentHarness({ reply }: { reply: ConnectSessionResponse }) {
  const host = useMemo(() => aHostAnswering(reply), [reply]);
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

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("useSessionAttachment — one connected state, whatever the reply routed to", () => {
  it("opens a media-capable connection when the reply names a room", () => {
    // Given
    cy.mount(<AttachmentHarness reply={ROOM_BACKED_CONNECT_OK} />);

    // When
    byTestId(CONNECT_BTN).click();

    // Then the attachment is simply connected, and what the session can do is the connection's to
    // say — the question the two statuses were being used to answer
    byTestId(STATUS_EL).should("have.text", "connected");
    byTestId(CAPABILITIES_EL).should("have.text", "media,presence,rpc");
    byTestId(ROOM_EL).should("have.text", A_ROOM);
  });

  it("opens an rpc-only connection when the reply names no room", () => {
    // Given a session the host serves itself — today's `connected-grpc`, and what a desktop app
    // over IPC produces
    cy.mount(<AttachmentHarness reply={HOST_SERVED_CONNECT_OK} />);

    // When
    byTestId(CONNECT_BTN).click();

    // Then it reaches the same connected state — not a lesser one — advertising plain RPC, and
    // carries no room for anything to try to join
    byTestId(STATUS_EL).should("have.text", "connected");
    byTestId(CAPABILITIES_EL).should("have.text", "rpc");
    byTestId(ROOM_EL).should("have.text", "");
  });
});
