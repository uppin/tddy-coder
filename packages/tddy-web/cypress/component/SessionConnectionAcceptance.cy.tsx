/**
 * Acceptance spec: one session connection, whatever the wire — and one handshake overlay over it.
 *
 * A session attached without LiveKit is a real, already-working configuration: the daemon serves
 * `terminal.TerminalService` against a PTY handle itself (`cli_session_manager.rs`). But it used to
 * land as `connected-grpc`, a second path consumers branched on and which never showed a connection
 * handshake overlay at all — the overlay was gated on `connected-livekit`. So the case that works
 * read to the operator as the case that never connected.
 *
 * Everything here runs through the **production** path: `LiveKitConnectionProvider.openSession`
 * decides room-or-no-room and hands back a real `openHostServedSession` connection, and
 * `SessionRuntime` is the component actually mounted. A spec that built its own connection object
 * would be asserting the behaviour of its own fixture — which is what the overlay claim, the one
 * thing here that is about a rendered surface, most needs not to be.
 *
 * Technical: `packages/tddy-web/docs/session-connections.md`
 * Stack: `optional-livekit` node 3 of 7.
 */

import React from "react";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import { create, toBinary } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ConnectionState, type Room } from "livekit-client";
import {
  ConnectionService,
  ListSessionsResponseSchema,
  SessionEntrySchema,
} from "../../src/gen/connection_pb";
import { TokenService, GenerateTokenResponseSchema } from "../../src/gen/token_pb";
import { daemonRpcIdentity } from "../../src/lib/participantRole";
import { SessionRuntime } from "../../src/components/sessions/SessionRuntime";
import { LiveKitConnectionProvider } from "../../src/rpc/connections/liveKit";
import { attachmentHintFromReply } from "../../src/rpc/connections/sessionAttachment";
import type { SessionAttachmentHint, SessionConnection } from "../../src/rpc/connections/session";
import type { HostConnection } from "../../src/rpc/connections/types";
import { useConnectionStatus } from "../../src/rpc/connections/useConnectionStatus";
import { toArrayBuffer } from "../support/rpc/protoRpc";
import { aSessionConnection } from "../support/rpc/sessionConnections";
import { byTestId, TEST_IDS } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const THIS_HOST = "instance-this-host";
const A_SESSION = "session-0001";
const ANOTHER_SESSION = "session-0002";
const A_SESSION_TOKEN = "an-operator-access-token";

/** What the daemon replies for a session it serves itself — no room, no participant. */
const A_HOST_SERVED_REPLY = { livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" };

/** What it replies for a session published into a room of its own. Nothing answers the url. */
const A_ROOM_BACKED_REPLY = {
  livekitRoom: "daemon-session-0001",
  livekitUrl: "ws://127.0.0.1:9999",
  livekitServerIdentity: `daemon-${THIS_HOST}-${A_SESSION}`,
};

const GENERATE_TOKEN_OK = toArrayBuffer(
  toBinary(
    GenerateTokenResponseSchema,
    create(GenerateTokenResponseSchema, { token: "lk-browser-token", ttlSeconds: BigInt(600) }),
  ),
);

const LIST_SESSIONS_OK = toArrayBuffer(
  toBinary(
    ListSessionsResponseSchema,
    create(ListSessionsResponseSchema, {
      sessions: [create(SessionEntrySchema, { sessionId: A_SESSION })],
    }),
  ),
);

/**
 * The common room the provider is bound to, holding exactly what a host connection reads off one.
 *
 * A host-served session's status is the *host's*, read through — a session cannot be more reachable
 * than the daemon serving it — so moving this room is how a spec moves the session's handshake.
 */
function aCommonRoom(state: ConnectionState, hostsOnIt: string[]) {
  const room = {
    state,
    remoteParticipants: new Map(
      hostsOnIt.map((id) => [daemonRpcIdentity(id), { identity: daemonRpcIdentity(id) }]),
    ),
  };
  return {
    asRoom: room as unknown as Room,
    /** The daemon comes up on the room — what the operator is waiting for behind the overlay. */
    admit(hostId: string) {
      room.state = ConnectionState.Connected;
      room.remoteParticipants.set(daemonRpcIdentity(hostId), { identity: daemonRpcIdentity(hostId) });
    },
  };
}

/**
 * A host reached over `room`, through the real provider.
 *
 * Its transport is this page's own `/rpc`, so a call issued on a session connection reaches a
 * `cy.intercept` rather than a stand-in — the provider is registered with no session resources
 * because nothing here names a room, which is itself the case under test.
 */
function aHostOn(room: Room): HostConnection {
  const transport = createConnectTransport({
    baseUrl: `${window.location.origin}/rpc`,
    useBinaryFormat: true,
  });
  const provider = new LiveKitConnectionProvider(room, () => transport, null);
  const host = provider.connectHost(THIS_HOST);
  if (!host) throw new Error("a provider holding a room must claim every host asked of it");
  return host;
}

/** `sessionId` opened on `host` from a reply naming no room — the production `openSession` path. */
function aHostServedSessionOn(host: HostConnection, sessionId: string): SessionConnection {
  return host.openSession(sessionId, attachmentHintFromReply(sessionId, A_HOST_SERVED_REPLY));
}

// ---------------------------------------------------------------------------
// Probes
// ---------------------------------------------------------------------------

const STATUSES_EL = "session-statuses";
const CAPABILITIES_EL = "session-capabilities";
const CALL_EL = "session-call";
const REFUSALS_EL = "session-refusals";

/** Reports each connection's status and capabilities, and can detach one. */
function SessionProbe({ connections }: { connections: SessionConnection[] }) {
  const [refusals, setRefusals] = React.useState<string[]>([]);

  const callEach = () =>
    setRefusals(
      connections.map((c) => {
        try {
          c.clientFor(ConnectionService);
          return `${c.sessionId}:served`;
        } catch {
          return `${c.sessionId}:refused`;
        }
      }),
    );

  return (
    <div>
      <div data-testid={STATUSES_EL}>
        {connections.map((c) => `${c.sessionId}:${c.status}`).join(",") || "none"}
      </div>
      <div data-testid={CAPABILITIES_EL}>
        {connections[0] ? [...connections[0].capabilities].sort().join(",") : "none"}
      </div>
      <div data-testid={REFUSALS_EL}>{refusals.join(",") || "none"}</div>
      <button
        data-testid="detach-first"
        onClick={() => {
          connections[0]?.close();
          callEach();
        }}
      >
        detach
      </button>
    </div>
  );
}

/** Calls a session's own client and shows what came back. */
function SessionCallProbe({ connection }: { connection: SessionConnection }) {
  const [label, setLabel] = React.useState("no answer yet");

  React.useEffect(() => {
    void connection
      .clientFor(ConnectionService)
      .listSessions({})
      .then((res) => setLabel(`sessions: ${res.sessions.length}`));
  }, [connection]);

  return <div data-testid={CALL_EL}>{label}</div>;
}

const CONNECTION_STATUS_EL = "probe-connection-status";

/**
 * The production runtime for one attached session — the surface the overlay actually covers —
 * alongside what the session's *connection* is saying at the same moment.
 *
 * Showing both is what makes a disagreement assertable: "the overlay is up" only means something
 * when the connection underneath it is known to be connected.
 */
function AttachedSessionRuntime({
  connection,
  hint,
}: {
  connection: SessionConnection;
  hint?: SessionAttachmentHint;
}) {
  const observed = useConnectionStatus(connection);
  return (
    <div style={{ height: "400px" }}>
      <div data-testid={CONNECTION_STATUS_EL}>{observed.status}</div>
      <SessionRuntime
        runtime={{
          sessionId: connection.sessionId,
          attached: true,
          connection,
          hint: hint ?? { sessionId: connection.sessionId },
          bytesIn: 0,
          bytesOut: 0,
          lastDataReceivedAt: null,
        }}
        focused
        sessionToken={A_SESSION_TOKEN}
      />
    </div>
  );
}

/**
 * A session room that joins without a media server.
 *
 * The connection's own join is not what these specs are about — they are about what the overlay
 * does once it has landed — so the room settles immediately instead of reaching anything. It carries
 * the roster listeners as well: the terminal a runtime opens on such a session watches the room for
 * the participant serving its PTY, and a double that answered no `on`/`off` would fail the mount
 * rather than the assertion.
 */
function aRoomThatJoinsAtOnce(): Room {
  const room = {
    state: ConnectionState.Disconnected,
    remoteParticipants: new Map<string, { identity: string }>(),
    on: () => room,
    off: () => room,
    connect: async () => {
      room.state = ConnectionState.Connected;
    },
    disconnect: async () => {
      room.state = ConnectionState.Disconnected;
    },
  };
  return room as unknown as Room;
}

/** A host that can also open room-backed sessions, joining `sessionRoom` for each. */
function aHostServingRooms(commonRoom: Room, sessionRoom: Room): HostConnection {
  const transport = createConnectTransport({
    baseUrl: `${window.location.origin}/rpc`,
    useBinaryFormat: true,
  });
  const provider = new LiveKitConnectionProvider(commonRoom, () => transport, {
    tokens: createClient(TokenService, transport),
    newRoom: () => sessionRoom,
  });
  const host = provider.connectHost(THIS_HOST);
  if (!host) throw new Error("a provider holding a room must claim every host asked of it");
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

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("a session connection on a host that serves its own session RPC", () => {
  it("reaches a connected status, which the gRPC path never showed at all", () => {
    // Given a host whose sessions are plain RPC — the desktop app over IPC, and yesterday's
    // `connected-grpc` case
    const host = aHostOn(aCommonRoom(ConnectionState.Connected, [THIS_HOST]).asRoom);

    cy.mount(<SessionProbe connections={[aHostServedSessionOn(host, A_SESSION)]} />);

    // Then it reports a real connection status, read through from the host that serves it
    byTestId(STATUSES_EL).should("have.text", `${A_SESSION}:connected`);
  });

  it("advertises rpc only, so the media surfaces do not apply to it", () => {
    const host = aHostOn(aCommonRoom(ConnectionState.Connected, [THIS_HOST]).asRoom);

    cy.mount(<SessionProbe connections={[aHostServedSessionOn(host, A_SESSION)]} />);

    // Then a consumer asks the connection what it can do rather than which wire it is on
    byTestId(CAPABILITIES_EL).should("have.text", "rpc");
  });

  it("serves the session's own RPC through the connection", () => {
    // Given the daemon answering a session-scoped call
    cy.intercept("POST", "**/rpc/connection.ConnectionService/ListSessions", (req) => {
      req.reply({
        statusCode: 200,
        headers: { "Content-Type": "application/proto" },
        body: LIST_SESSIONS_OK,
      });
    }).as("listSessions");
    const host = aHostOn(aCommonRoom(ConnectionState.Connected, [THIS_HOST]).asRoom);

    cy.mount(<SessionCallProbe connection={aHostServedSessionOn(host, A_SESSION)} />);

    // Then the call travelled over the host's own wire — no second one was opened for a session
    // that never named a room
    cy.wait("@listSessions");
    byTestId(CALL_EL).should("have.text", "sessions: 1");
  });
});

describe("the handshake overlay over an attached session's panes", () => {
  it("covers the panes of a session whose host is not reachable yet", () => {
    // Given a session on a daemon that has not come up on the common room
    const common = aCommonRoom(ConnectionState.Disconnected, []);
    const connection = aHostServedSessionOn(aHostOn(common.asRoom), A_SESSION);

    cy.mount(<AttachedSessionRuntime connection={connection} />);

    // Then the operator is told, on a wire that used to show nothing at all — this is the case that
    // rendered an inert, silent pane
    byTestId(TEST_IDS.sessionConnectionOverlay).should("exist").and("contain.text", "Connecting");
  });

  it("clears once the connection is up, so the panes become interactive", () => {
    // Given a session waiting behind the overlay
    const common = aCommonRoom(ConnectionState.Disconnected, []);
    const connection = aHostServedSessionOn(aHostOn(common.asRoom), A_SESSION);
    cy.mount(<AttachedSessionRuntime connection={connection} />);
    byTestId(TEST_IDS.sessionConnectionOverlay).should("exist");

    // When the daemon comes up
    cy.then(() => common.admit(THIS_HOST));

    // Then the overlay goes. It is `absolute inset-0 pointer-events-auto`, so an overlay that could
    // not clear would leave a working terminal under a sheet swallowing every click, with nothing
    // the operator could do about it
    byTestId(TEST_IDS.sessionConnectionOverlay).should("not.exist");
  });

  it("does not lift because a session's own process is missing from the room", () => {
    // Given a reachable daemon that is not itself the session's process — no roster anywhere names
    // the participant this connection addresses
    const common = aCommonRoom(ConnectionState.Connected, [THIS_HOST]);
    const connection = aHostServedSessionOn(aHostOn(common.asRoom), A_SESSION);

    cy.mount(<AttachedSessionRuntime connection={connection} />);

    // Then the pane is interactive. Participant presence routes calls; it must never be able to
    // hold the overlay up, because an absent peer makes a call fail visibly while a stuck overlay
    // has no recovery at all
    byTestId(CONNECTION_STATUS_EL).should("have.text", "connected");
    byTestId(TEST_IDS.sessionConnectionOverlay).should("not.exist");
  });

  it("lifts on a room-backed session once its connection is up, because the terminal has no join of its own", () => {
    // Given a room-backed session whose connection has joined its room. The terminal used to make a
    // *second*, independent join of that same room, so the pane stayed covered until both had
    // landed; it now reads its bytes off this connection, and there is one handshake to wait for
    interceptGenerateToken();
    const transport = createConnectTransport({
      baseUrl: `${window.location.origin}/rpc`,
      useBinaryFormat: true,
    });
    const host = aHostServingRooms(
      aCommonRoom(ConnectionState.Connected, [THIS_HOST]).asRoom,
      aRoomThatJoinsAtOnce(),
    );
    const hint = attachmentHintFromReply(A_SESSION, A_ROOM_BACKED_REPLY);
    const connection = host.openSession(A_SESSION, hint);

    cy.mount(<AttachedSessionRuntime connection={connection} hint={hint} />);

    // Then the connection is connected and the pane is interactive. Keeping the overlay up here
    // would now be waiting on a handshake nobody is performing
    byTestId(CONNECTION_STATUS_EL).should("have.text", "connected");
    byTestId(TEST_IDS.sessionConnectionOverlay).should("not.exist");
  });

  it("says so when the connection failed rather than sitting on Connecting", () => {
    // Given a session whose connection gave a verdict — the daemon refused to mint a browser token
    // for its room, which is a thing that has happened rather than a thing still happening
    const connection = aSessionConnection(A_SESSION)
      .failedWith("browser is not authorised for this room")
      .servingOver(anInMemoryRpcBackend().transport())
      .build();

    cy.mount(<AttachedSessionRuntime connection={connection} />);

    // Then the overlay stays up and reads as a failure. An error left showing "Connecting…" is an
    // operator waiting indefinitely for something that already finished
    byTestId(TEST_IDS.sessionConnectionOverlay).should("exist");
    byTestId(TEST_IDS.sessionConnectionError).should("exist");
  });
});

describe("several sessions attached at once", () => {
  it("holds one connection per session, each routed on its own", () => {
    // Given two sessions attached together — several open terminals in the drawer
    const host = aHostOn(aCommonRoom(ConnectionState.Connected, [THIS_HOST]).asRoom);
    const connections = [
      aHostServedSessionOn(host, A_SESSION),
      aHostServedSessionOn(host, ANOTHER_SESSION),
    ];

    cy.mount(<SessionProbe connections={connections} />);

    // Then each is its own connection with its own claim — two attachments of one session are two
    // attachments, so `openSession` deliberately memoises nothing
    byTestId(STATUSES_EL).should(
      "have.text",
      `${A_SESSION}:connected,${ANOTHER_SESSION}:connected`,
    );
    cy.wrap(null).should(() => expect(connections[0]).to.not.equal(connections[1]));
  });

  it("releases only the detached session, leaving the others serving", () => {
    // Given two attached sessions
    const host = aHostOn(aCommonRoom(ConnectionState.Connected, [THIS_HOST]).asRoom);
    const connections = [
      aHostServedSessionOn(host, A_SESSION),
      aHostServedSessionOn(host, ANOTHER_SESSION),
    ];
    cy.mount(<SessionProbe connections={connections} />);

    // When one is detached
    byTestId("detach-first").click();

    // Then exactly that one refuses to issue calls, and the other carries on. A close that took the
    // host's shared wire with it would detach every other session on the same daemon
    byTestId(REFUSALS_EL).should(
      "have.text",
      `${A_SESSION}:refused,${ANOTHER_SESSION}:served`,
    );
  });
});
