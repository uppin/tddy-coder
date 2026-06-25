/**
 * High-level scenario helpers for ConnectionScreen / AppRouting RPC intercepts.
 *
 * Consolidates the five near-identical `interceptAllRpcs*` variants that lived in
 * ConnectionScreen.cy.tsx into one parameterised function.
 *
 * Usage:
 *
 *   interceptConnectionRpcs([ACTIVE_SESSION]);
 *   interceptConnectionRpcs([ACTIVE_SESSION, INACTIVE_SESSION], { daemons: [DAEMON_LOCAL] });
 *   interceptConnectionRpcs(sessions, { onDeleteSession: (ids) => capturedIds.push(...ids) });
 *   interceptConnectionRpcs(sessions, { listSessionsFactory: () => dynamicRows });
 *   interceptConnectionRpcs(sessions, { projectIdCollision: [PROJECT_WS, PROJECT_SRV] });
 */

import { create, fromBinary, toBinary } from "@bufbuild/protobuf";

import {
  ConnectSessionRequestSchema,
  ConnectSessionResponseSchema,
  DeleteSessionRequestSchema,
  ResumeSessionResponseSchema,
  StartSessionResponseSchema,
  type ProjectEntry,
  type SessionEntry,
} from "../../../src/gen/connection_pb";
import { toArrayBuffer, decodeProtoRequestBody } from "./protoRpc";
import {
  anAuthStatusAuthenticated,
  aConnectSessionResponse,
  aGenerateTokenResponse,
  aRefreshTokenResponse,
  listDefaultAgents,
  listAgents,
  listEligibleDaemons,
  listProjectBranches,
  listProjects,
  listSessions,
  listTools,
  OK_RESPONSE_BYTES,
} from "./responses";

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

export interface DaemonEntry {
  instanceId: string;
  label: string;
  isLocal: boolean;
}

export interface ConnectionRpcOptions {
  /**
   * Override the ListEligibleDaemons response.
   * When omitted, returns a single local daemon `{ instanceId: "local", isLocal: true }`.
   */
  daemons?: DaemonEntry[];

  /**
   * Override the ListAgents response.
   * When omitted, returns DEFAULT_AGENTS (the dev.daemon.yaml set).
   */
  agents?: Array<{ id: string; label: string }>;

  /**
   * When provided, DeleteSession decodes the request body and appends the sessionId here,
   * allowing tests to assert which sessions were deleted.
   */
  captureDeleteSessionIds?: string[];

  /**
   * When provided, ListSessions calls this function on each request (for poll-driven tests).
   * The static `sessions` argument is ignored when this is set.
   */
  listSessionsFactory?: () => Partial<SessionEntry>[];

  /**
   * When provided, ListProjects returns these entries verbatim (used for the
   * same-projectId-across-daemons collision scenario).
   */
  projectsOverride?: Partial<ProjectEntry>[];
}

// ---------------------------------------------------------------------------
// Main interceptor
// ---------------------------------------------------------------------------

/**
 * Registers cy.intercept aliases for all six RPCs used by ConnectionScreen:
 * GetAuthStatus, ListTools, ListAgents, ListEligibleDaemons, ListSessions, ListProjects.
 *
 * Always sets aliases @getAuthStatus, @listTools, @listAgents, @listEligibleDaemons,
 * @listSessions, @listProjects.
 */
export function interceptConnectionRpcs(
  sessions: Partial<SessionEntry>[],
  opts: ConnectionRpcOptions = {},
): void {
  // Auth
  const authBody = toArrayBuffer(anAuthStatusAuthenticated());
  cy.intercept("POST", "**/rpc/auth.AuthService/GetAuthStatus", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: authBody });
  }).as("getAuthStatus");

  // Tools
  const toolsBody = toArrayBuffer(listTools());
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListTools", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: toolsBody });
  }).as("listTools");

  // Agents
  const agentsBody = toArrayBuffer(
    opts.agents === undefined ? listDefaultAgents() : listAgents(opts.agents),
  );
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListAgents", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: agentsBody });
  }).as("listAgents");

  // Eligible daemons
  const daemonsBody = toArrayBuffer(
    opts.daemons === undefined
      ? listEligibleDaemons([{ instanceId: "local", label: "local (this daemon)", isLocal: true }])
      : listEligibleDaemons(opts.daemons),
  );
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListEligibleDaemons", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: daemonsBody });
  }).as("listEligibleDaemons");

  // Sessions — either factory (dynamic) or static body
  if (opts.listSessionsFactory) {
    const factory = opts.listSessionsFactory;
    cy.intercept("POST", "**/rpc/connection.ConnectionService/ListSessions", (req) => {
      const body = toArrayBuffer(listSessions(factory()));
      req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body });
    }).as("listSessions");
  } else {
    const sessionsBody = toArrayBuffer(listSessions(sessions));
    cy.intercept("POST", "**/rpc/connection.ConnectionService/ListSessions", (req) => {
      req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: sessionsBody });
    }).as("listSessions");
  }

  // Projects — explicit override, daemon-derived, or default
  let projectsBody: ArrayBuffer;
  if (opts.projectsOverride !== undefined) {
    projectsBody = toArrayBuffer(listProjects(opts.projectsOverride));
  } else if (opts.daemons !== undefined) {
    const localDaemon = opts.daemons.find((d) => d.isLocal) ?? opts.daemons[0];
    projectsBody = toArrayBuffer(
      listProjects([{ daemonInstanceId: localDaemon?.instanceId ?? "" }]),
    );
  } else {
    projectsBody = toArrayBuffer(listProjects([{}]));
  }
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListProjects", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: projectsBody });
  }).as("listProjects");

  // DeleteSession — always registered; optionally captures decoded session ids
  if (opts.captureDeleteSessionIds !== undefined) {
    const captureIds = opts.captureDeleteSessionIds;
    cy.intercept("POST", "**/rpc/connection.ConnectionService/DeleteSession", (req) => {
      const u8 = decodeProtoRequestBody(req.body);
      const decoded = fromBinary(DeleteSessionRequestSchema, u8);
      captureIds.push(decoded.sessionId);
      req.reply({
        statusCode: 200,
        headers: { "Content-Type": "application/proto" },
        body: toArrayBuffer(OK_RESPONSE_BYTES),
      });
    }).as("deleteSession");
  } else {
    cy.intercept("POST", "**/rpc/connection.ConnectionService/DeleteSession", (req) => {
      req.reply({
        statusCode: 200,
        headers: { "Content-Type": "application/proto" },
        body: toArrayBuffer(OK_RESPONSE_BYTES),
      });
    }).as("deleteSession");
  }
}

// ---------------------------------------------------------------------------
// Collision scenario helper
// ---------------------------------------------------------------------------

/**
 * Canonical daemon pair for multi-host tests.
 */
export const DAEMON_LOCAL: DaemonEntry = {
  instanceId: "workstation-1",
  label: "workstation-1 (this daemon)",
  isLocal: true,
};

export const DAEMON_PEER: DaemonEntry = {
  instanceId: "server-2",
  label: "server-2",
  isLocal: false,
};

/**
 * The projectId used in collision tests — same project ID across two daemons.
 * Exported so test files can reference it without re-declaring.
 */
export const COLLISION_PROJECT_ID = "cccccccc-dddd-4eee-8fff-999999999999";

/**
 * Registers connection RPCs with ListProjects returning two entries sharing the same
 * projectId across `DAEMON_LOCAL` and `DAEMON_PEER` (the project-id collision scenario).
 */
export function interceptConnectionRpcsProjectIdCollision(
  sessions: Partial<SessionEntry>[],
): void {
  interceptConnectionRpcs(sessions, {
    daemons: [DAEMON_LOCAL, DAEMON_PEER],
    projectsOverride: [
      {
        projectId: COLLISION_PROJECT_ID,
        name: "dup-workstation",
        gitUrl: "https://github.com/test/dup.git",
        mainRepoPath: "/home/ws/dup",
        daemonInstanceId: DAEMON_LOCAL.instanceId,
      },
      {
        projectId: COLLISION_PROJECT_ID,
        name: "dup-server",
        gitUrl: "https://github.com/test/dup.git",
        mainRepoPath: "/srv/dup",
        daemonInstanceId: DAEMON_PEER.instanceId,
      },
    ],
  });
}

// ---------------------------------------------------------------------------
// Individual interceptors (reusable in tests that only need one RPC)
// ---------------------------------------------------------------------------

/** Intercept ConnectSession and reply with a static room. */
export function interceptConnectSession(
  overrides: { livekitRoom?: string; livekitUrl?: string; livekitServerIdentity?: string } = {},
): void {
  const body = toArrayBuffer(aConnectSessionResponse(overrides));
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ConnectSession", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body });
  }).as("connectSession");
}

/**
 * Intercept ConnectSession and derive the room name from the requested sessionId.
 * Used for multi-session tests where each session gets its own LiveKit room.
 */
export function interceptConnectSessionPerSessionId(): void {
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ConnectSession", (req) => {
    const bodyBytes = new Uint8Array(req.body as ArrayBuffer);
    const decoded = fromBinary(ConnectSessionRequestSchema, bodyBytes);
    const sid = decoded.sessionId;
    const body = toArrayBuffer(
      toBinary(
        ConnectSessionResponseSchema,
        create(ConnectSessionResponseSchema, {
          livekitRoom: `room-${sid}`,
          livekitUrl: "ws://127.0.0.1:7880",
          livekitServerIdentity: `server-${sid.slice(0, 8)}`,
        }),
      ),
    );
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body });
  }).as("connectSessionMulti");
}

/** Intercept ResumeSession with an optional sessionId override. */
export function interceptResumeSession(sessionId: string): void {
  const body = toArrayBuffer(
    toBinary(
      ResumeSessionResponseSchema,
      create(ResumeSessionResponseSchema, {
        sessionId,
        livekitRoom: "resume-room-ct",
        livekitUrl: "ws://127.0.0.1:7880",
        livekitServerIdentity: "server",
      }),
    ),
  );
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ResumeSession", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body });
  }).as("resumeSession");
}

/** Intercept StartSession with a given sessionId. */
export function interceptStartSession(sessionId: string): void {
  const responseBody = toArrayBuffer(
    toBinary(
      StartSessionResponseSchema,
      create(StartSessionResponseSchema, {
        sessionId,
        livekitRoom: `room-${sessionId}`,
        livekitUrl: "ws://127.0.0.1:7880",
        livekitServerIdentity: "server-new",
      }),
    ),
  );
  // Use middleware: true so this handler runs BEFORE any non-middleware handler (e.g. an
  // anonymous capturing handler registered after this call in the test).
  //
  // Calling req.on('before:response', ...) subscribes a response modifier WITHOUT setting
  // stopPropagation — so Cypress falls through to the next (non-middleware, anonymous) handler
  // rather than skipping it. The anonymous handler can then capture the request body and call
  // req.continue() to send the request to the upstream server (Vite dev server). When the Vite
  // 404 response comes back, our before:response modifier replaces it with the valid protobuf
  // response, letting the component resolve the RPC successfully.
  //
  // This avoids the problem where req.continue() / req.reply() set stopPropagation=true and
  // skip subsequent handlers (confirmed from Cypress runner source: finish(true) skips, finish(false)
  // propagates).
  cy.intercept(
    "**/rpc/connection.ConnectionService/StartSession",
    { method: "POST", middleware: true },
    (req) => {
      req.on("before:response", (res) => {
        // Replace whatever upstream response arrives (e.g. 404 from Vite dev server) with the
        // canned proto response so that the component's startSession() resolves successfully.
        res.send({
          statusCode: 200,
          headers: { "Content-Type": "application/proto" },
          body: responseBody,
        });
      });
      // Do NOT call req.reply() or req.continue() here — returning without either sets
      // stopPropagation=false, letting Cypress continue to the next registered handler.
    },
  ).as("startSession");
}

/** Intercept ListProjectBranches and reply with the given branch names. */
export function interceptListProjectBranches(branches: string[] = []): void {
  const body = toArrayBuffer(listProjectBranches(branches));
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListProjectBranches", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body });
  }).as("listProjectBranches");
}

/** Intercept SignalSession and reply with ok:true. */
export function interceptSignalSession(): void {
  cy.intercept("POST", "**/rpc/connection.ConnectionService/SignalSession", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(OK_RESPONSE_BYTES),
    });
  }).as("signalSession");
}

/** Intercept SignalSession and reply with a 412 error. */
export function interceptSignalSessionError(): void {
  cy.intercept("POST", "**/rpc/connection.ConnectionService/SignalSession", (req) => {
    req.reply({
      statusCode: 412,
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ code: "failed_precondition", message: "Process not alive" }),
    });
  }).as("signalSessionError");
}

/** Intercept DeleteSession and reply with ok:true. Decodes and captures the session id. */
export function interceptDeleteSession(captureIds: string[]): void {
  cy.intercept("POST", "**/rpc/connection.ConnectionService/DeleteSession", (req) => {
    const u8 = decodeProtoRequestBody(req.body);
    const decoded = fromBinary(DeleteSessionRequestSchema, u8);
    captureIds.push(decoded.sessionId);
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(OK_RESPONSE_BYTES),
    });
  }).as("deleteSession");
}

/** Intercept GenerateToken + RefreshToken for presence. */
export function interceptTokenForPresence(): void {
  const generateBody = toArrayBuffer(aGenerateTokenResponse());
  cy.intercept("POST", "**/rpc/token.TokenService/GenerateToken", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: generateBody });
  }).as("generateTokenPresence");

  const refreshBody = toArrayBuffer(aRefreshTokenResponse());
  cy.intercept("POST", "**/rpc/token.TokenService/RefreshToken", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: refreshBody });
  }).as("refreshTokenPresence");
}
