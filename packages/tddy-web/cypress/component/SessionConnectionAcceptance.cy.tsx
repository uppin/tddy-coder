/**
 * Acceptance spec: one session connection, whatever the wire.
 *
 * A session attached without LiveKit is a real, already-working configuration — the daemon serves
 * `terminal.TerminalService` against a PTY handle itself (`cli_session_manager.rs`). But today it
 * lands as `connected-grpc`, a second path that consumers branch on and that never shows a
 * connection handshake overlay at all: `SessionRuntime.tsx:130` gates the overlay on
 * `connected-livekit`. So the case that works reads to the user as the case that never connected.
 *
 * These specs pin the replacement: one connected status, capabilities on the side, and a real
 * status on every wire.
 *
 * Changeset: `docs/dev/1-WIP/2026-09-05-optional-livekit-session-connection.md`
 * Stack: `optional-livekit` node 3 of 7.
 */

import React from "react";
import { createClient, type Client } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  ConnectionService,
  ListSessionsResponseSchema,
  SessionEntrySchema,
} from "../../src/gen/connection_pb";
import { attachmentHintFromReply } from "../../src/rpc/connections/sessionAttachment";
import type { SessionAttachmentHint, SessionConnection } from "../../src/rpc/connections/session";
import type {
  ConnectionCapability,
  ConnectionProvider,
  HostConnection,
} from "../../src/rpc/connections/types";
import { byTestId } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const THIS_HOST = "instance-this-host";
const A_SESSION = "session-0001";
const ANOTHER_SESSION = "session-0002";

/** What the daemon replies for a session it serves itself — no room, no participant. */
const A_HOST_SERVED_REPLY = { livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" };

/**
 * A provider whose sessions are plain RPC and which records every open and close, so a test can
 * assert that detaching releases a connection rather than merely forgetting it.
 */
function aRecordingProvider(backend: ReturnType<typeof anInMemoryRpcBackend>) {
  const transport = backend.transport();
  const opened: string[] = [];
  const closed: string[] = [];

  const clientCache = new Map<DescService, Client<DescService>>();
  const clientFor = <S extends DescService>(service: S): Client<S> => {
    const cached = clientCache.get(service);
    if (cached) return cached as Client<S>;
    const built = createClient(service, transport);
    clientCache.set(service, built as Client<DescService>);
    return built;
  };

  const provider: ConnectionProvider = {
    id: "recording",
    connectHost: (hostId) => {
      if (hostId !== THIS_HOST) return null;
      const host: HostConnection = {
        hostId,
        providerId: "recording",
        status: "connected",
        error: null,
        capabilities: new Set<ConnectionCapability>(["rpc"]),
        clientFor,
        transport: () => transport,
        openSession: (sessionId: string, _hint: SessionAttachmentHint): SessionConnection => {
          opened.push(sessionId);
          let live = true;
          return {
            hostId,
            sessionId,
            status: "connected",
            error: null,
            capabilities: new Set<ConnectionCapability>(["rpc"]),
            clientFor: <S extends DescService>(service: S): Client<S> => {
              if (!live) throw new Error(`session ${sessionId} is closed`);
              return clientFor(service);
            },
            transport: () => transport,
            close: () => {
              if (!live) return;
              live = false;
              closed.push(sessionId);
            },
          };
        },
      };
      return host;
    },
  };

  return { provider, opened, closed };
}

/**
 * The host connection the provider issues, taken **directly** rather than through
 * `ConnectionProviderRegistry`.
 *
 * The registry is node 1's (`connection-model`) and is still unimplemented on this branch. It is
 * also incidental to everything these specs assert — they are about what a session connection is,
 * not about how a host is resolved to one. Injecting at this seam keeps every failure here
 * attributable to this node's own missing implementation.
 */
function aHostConnectionFrom(provider: ConnectionProvider): HostConnection {
  const host = provider.connectHost(THIS_HOST);
  if (!host) throw new Error("the fixture provider must claim the test host");
  return host;
}

// ---------------------------------------------------------------------------
// Probes
// ---------------------------------------------------------------------------

/** Attaches `sessionIds`, reports each connection's status and capabilities, and can detach one. */
function SessionProbe({ host, sessionIds }: { host: HostConnection; sessionIds: string[] }) {
  const [detached, setDetached] = React.useState<string[]>([]);
  const connections = React.useMemo(
    () => sessionIds.map((id) => host.openSession(id, attachmentHintFromReply(id, A_HOST_SERVED_REPLY))),
    [host, sessionIds],
  );

  return (
    <div>
      <div data-testid="session-statuses">
        {connections.map((c) => `${c.sessionId}:${c.status}`).join(",") || "none"}
      </div>
      <div data-testid="session-capabilities">
        {connections[0] ? [...connections[0].capabilities].sort().join(",") : "none"}
      </div>
      <div data-testid="detached">{detached.join(",") || "none"}</div>
      <button
        data-testid="detach-first"
        onClick={() => {
          connections[0]?.close();
          setDetached((d) => [...d, connections[0]?.sessionId ?? ""]);
        }}
      >
        detach
      </button>
    </div>
  );
}

/** Calls a session's own client and shows what came back. */
function SessionCallProbe({ host }: { host: HostConnection }) {
  const [label, setLabel] = React.useState("no session");

  React.useEffect(() => {
    const session = host.openSession(
      A_SESSION,
      attachmentHintFromReply(A_SESSION, A_HOST_SERVED_REPLY),
    );
    let cancelled = false;
    void session
      .clientFor(ConnectionService)
      .listSessions({})
      .then((res) => {
        if (!cancelled) setLabel(`sessions: ${res.sessions.length}`);
      });
    return () => {
      cancelled = true;
      session.close();
    };
  }, [host]);

  return <div data-testid="session-call">{label}</div>;
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("a session connection on a host that serves its own session RPC", () => {
  it("reaches a connected status, which the gRPC path never showed at all", () => {
    // Given a host whose sessions are plain RPC — the desktop app over IPC, and today's
    // `connected-grpc` case
    const { provider } = aRecordingProvider(anInMemoryRpcBackend());

    cy.mount(<SessionProbe host={aHostConnectionFrom(provider)} sessionIds={[A_SESSION]} />);

    // Then it reports a real connection status. Today `SessionRuntime` gates its handshake overlay
    // on `connected-livekit`, so this session shows no connection state whatsoever.
    byTestId("session-statuses").should("have.text", `${A_SESSION}:connected`);
  });

  it("advertises rpc only, so the media surfaces do not apply to it", () => {
    const { provider } = aRecordingProvider(anInMemoryRpcBackend());

    cy.mount(<SessionProbe host={aHostConnectionFrom(provider)} sessionIds={[A_SESSION]} />);

    // Then a consumer asks the connection what it can do rather than which wire it is on
    byTestId("session-capabilities").should("have.text", "rpc");
  });

  it("serves the session's own RPC through the connection", () => {
    const backend = anInMemoryRpcBackend().onUnary(ConnectionService.method.listSessions, () =>
      create(ListSessionsResponseSchema, {
        sessions: [create(SessionEntrySchema, { sessionId: A_SESSION })],
      }),
    );
    const { provider } = aRecordingProvider(backend);

    cy.mount(<SessionCallProbe host={aHostConnectionFrom(provider)} />);

    byTestId("session-call").should("have.text", "sessions: 1");
  });
});

describe("several sessions attached at once", () => {
  it("holds one connection per session", () => {
    // Given two sessions attached together — several open terminals in the drawer
    const { provider, opened } = aRecordingProvider(anInMemoryRpcBackend());

    cy.mount(<SessionProbe host={aHostConnectionFrom(provider)} sessionIds={[A_SESSION, ANOTHER_SESSION]} />);

    // Then each has its own connection, created the same way a LiveKit room and participant are
    // created for a session
    byTestId("session-statuses").should(
      "have.text",
      `${A_SESSION}:connected,${ANOTHER_SESSION}:connected`,
    );
    cy.wrap(null).should(() => expect(opened).to.deep.equal([A_SESSION, ANOTHER_SESSION]));
  });

  it("releases only the detached session, leaving the others serving", () => {
    // Given two attached sessions
    const { provider, closed } = aRecordingProvider(anInMemoryRpcBackend());

    cy.mount(<SessionProbe host={aHostConnectionFrom(provider)} sessionIds={[A_SESSION, ANOTHER_SESSION]} />);

    // When one is detached
    byTestId("detach-first").click();

    // Then exactly that one is released. On a multi-connection transport this is a real resource,
    // not a forgotten reference — without it every attach leaks a host-side peer.
    byTestId("detached").should("have.text", A_SESSION);
    cy.wrap(null).should(() => expect(closed).to.deep.equal([A_SESSION]));
  });
});
