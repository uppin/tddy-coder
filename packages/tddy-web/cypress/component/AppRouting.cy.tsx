import React from "react";
import { create, toBinary } from "@bufbuild/protobuf";
import { App } from "../../src/index";
import {
  ListToolsResponseSchema,
  ToolInfoSchema,
  ListAgentsResponseSchema,
  AgentInfoSchema,
  ListSessionsResponseSchema,
  SessionEntrySchema,
  ListProjectsResponseSchema,
  ProjectEntrySchema,
  ListEligibleDaemonsResponseSchema,
  EligibleDaemonEntrySchema,
  ConnectSessionResponseSchema,
} from "../../src/gen/connection_pb";
import { GetAuthStatusResponseSchema, GitHubUserSchema } from "../../src/gen/auth_pb";
import { GenerateTokenResponseSchema, RefreshTokenResponseSchema } from "../../src/gen/token_pb";
import { TERMINAL_SESSION_ROUTE_PREFIX } from "../../src/routing/appRoutes";

const toArrayBuffer = (u8: Uint8Array) => {
  const buf = new ArrayBuffer(u8.length);
  new Uint8Array(buf).set(u8);
  return buf;
};

const ACTIVE_SESSION = {
  sessionId: "session-active-1",
  createdAt: "2026-03-21T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/project",
  pid: 12345,
  isActive: true,
  projectId: "proj-1",
};

const PROJECT = {
  projectId: "proj-1",
  name: "Test Project",
  gitUrl: "https://github.com/test/project.git",
  mainRepoPath: "/home/dev/project",
};

const MOCK_DEFAULT_LIST_AGENTS = [
  { id: "claude", label: "Claude (opus)" },
  { id: "stub", label: "Stub" },
];

type MockSessionRow = {
  sessionId: string;
  createdAt: string;
  status: string;
  repoPath: string;
  pid: number;
  isActive: boolean;
  projectId: string;
};

function mockAuthAuthenticated() {
  const user = create(GitHubUserSchema, {
    login: "testuser",
    avatarUrl: "https://example.com/avatar.png",
    name: "Test User",
    id: BigInt(42),
  });
  return toBinary(
    GetAuthStatusResponseSchema,
    create(GetAuthStatusResponseSchema, { authenticated: true, user }),
  );
}

function mockListToolsResponse() {
  return toBinary(
    ListToolsResponseSchema,
    create(ListToolsResponseSchema, {
      tools: [create(ToolInfoSchema, { path: "/usr/bin/tddy-coder", label: "tddy-coder" })],
    }),
  );
}

function mockListAgentsResponse(agents: Array<{ id: string; label: string }>) {
  return toBinary(
    ListAgentsResponseSchema,
    create(ListAgentsResponseSchema, {
      agents: agents.map((a) => create(AgentInfoSchema, { id: a.id, label: a.label })),
    }),
  );
}

function mockListSessionsResponse(sessions: MockSessionRow[]) {
  return toBinary(
    ListSessionsResponseSchema,
    create(ListSessionsResponseSchema, {
      sessions: sessions.map((s) => create(SessionEntrySchema, s)),
    }),
  );
}

function mockListProjectsResponse() {
  return toBinary(
    ListProjectsResponseSchema,
    create(ListProjectsResponseSchema, {
      projects: [create(ProjectEntrySchema, PROJECT)],
    }),
  );
}

function mockListEligibleDaemonsDefaultResponse() {
  return toBinary(
    ListEligibleDaemonsResponseSchema,
    create(ListEligibleDaemonsResponseSchema, {
      daemons: [
        create(EligibleDaemonEntrySchema, {
          instanceId: "local",
          label: "local (this daemon)",
          isLocal: true,
        }),
      ],
    }),
  );
}

function interceptDaemonAppRpcs(sessions: MockSessionRow[]) {
  const authBody = mockAuthAuthenticated();
  cy.intercept("POST", "**/rpc/auth.AuthService/GetAuthStatus", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(authBody),
    });
  }).as("getAuthStatus");

  const toolsBody = mockListToolsResponse();
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListTools", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(toolsBody),
    });
  }).as("listTools");

  const agentsBody = mockListAgentsResponse(MOCK_DEFAULT_LIST_AGENTS);
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListAgents", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(agentsBody),
    });
  }).as("listAgents");

  const daemonsBody = mockListEligibleDaemonsDefaultResponse();
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListEligibleDaemons", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(daemonsBody),
    });
  }).as("listEligibleDaemons");

  const sessionsBody = mockListSessionsResponse(sessions);
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListSessions", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(sessionsBody),
    });
  }).as("listSessions");

  const projectsBody = mockListProjectsResponse();
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListProjects", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(projectsBody),
    });
  }).as("listProjects");
}

function interceptConnectSessionSuccess() {
  const body = toBinary(
    ConnectSessionResponseSchema,
    create(ConnectSessionResponseSchema, {
      livekitRoom: "session-room-ct",
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: "server",
    }),
  );
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ConnectSession", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(body),
    });
  }).as("connectSession");
}

function interceptTokenForPresence() {
  const mockToken = "mock-jwt-presence";
  const mockTtl = 600;
  const generateBody = toBinary(
    GenerateTokenResponseSchema,
    create(GenerateTokenResponseSchema, {
      token: mockToken,
      ttlSeconds: BigInt(mockTtl),
    }),
  );
  const refreshBody = toBinary(
    RefreshTokenResponseSchema,
    create(RefreshTokenResponseSchema, {
      token: mockToken,
      ttlSeconds: BigInt(mockTtl),
    }),
  );
  cy.intercept("POST", "**/rpc/token.TokenService/GenerateToken", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(generateBody),
    });
  }).as("generateTokenPresence");
  cy.intercept("POST", "**/rpc/token.TokenService/RefreshToken", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(refreshBody),
    });
  }).as("refreshTokenPresence");
}

function interceptDaemonModeConfig() {
  cy.intercept("GET", "**/api/config", {
    statusCode: 200,
    headers: { "Content-Type": "application/json" },
    body: {
      daemon_mode: true,
      livekit_url: "ws://127.0.0.1:7880",
      common_room: "tddy-lobby",
    },
  }).as("apiConfigDaemon");
}

/** Wait until daemon ConnectionScreen has loaded (config + session list RPCs). */
function waitForDaemonSessionShell() {
  cy.contains("h3", "Projects", { timeout: 20000 }).should("be.visible");
}

describe("App routing (daemon mode, acceptance)", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    interceptDaemonModeConfig();
  });

  it("connect keeps home URL and shows overlay; Expand pushes dedicated terminal route", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptDaemonAppRpcs([ACTIVE_SESSION]);
    interceptConnectSessionSuccess();
    interceptTokenForPresence();
    cy.window().then((win) => {
      win.history.replaceState(null, "", "/");
    });
    cy.mount(<App />);
    waitForDaemonSessionShell();
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="connect-${ACTIVE_SESSION.sessionId}"]`, { timeout: 8000 }).click();
    cy.wait("@connectSession");
    cy.window().its("location.pathname").should("eq", "/");
    cy.get("[data-testid='terminal-reconnect-overlay-root']", { timeout: 15000 }).should("be.visible");
    cy.get("[data-testid='terminal-reconnect-expand']", { timeout: 15000 }).should("be.visible").click();
    cy.window()
      .its("location.pathname")
      .should("contain", `${TERMINAL_SESSION_ROUTE_PREFIX}/${ACTIVE_SESSION.sessionId}`);
  });

  it("browser_back_from_terminal_returns_to_session_list", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptDaemonAppRpcs([ACTIVE_SESSION]);
    interceptConnectSessionSuccess();
    interceptTokenForPresence();
    cy.window().then((win) => {
      win.history.replaceState(null, "", "/");
    });
    cy.mount(<App />);
    waitForDaemonSessionShell();
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="connect-${ACTIVE_SESSION.sessionId}"]`, { timeout: 8000 }).click();
    cy.wait("@connectSession");
    cy.get("[data-testid='terminal-reconnect-overlay-root']", { timeout: 8000 }).should("be.visible");
    cy.get("[data-testid='terminal-reconnect-expand']", { timeout: 8000 }).should("be.visible").click();
    cy.get("[data-testid='connected-terminal-container']", { timeout: 8000 }).should("exist");
    cy.window().then((win) => {
      win.history.back();
    });
    cy.get(`[data-testid="sessions-table-${PROJECT.projectId}"]`, { timeout: 8000 }).should("be.visible");
    cy.get("[data-testid='connected-terminal-container']").should("not.exist");
  });

  it("terminal_route_unknown_session_shows_error_and_navigation_home", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptDaemonAppRpcs([ACTIVE_SESSION]);
    interceptConnectSessionSuccess();
    interceptTokenForPresence();
    cy.window().then((win) => {
      win.history.replaceState(null, "", `${TERMINAL_SESSION_ROUTE_PREFIX}/does-not-exist-999`);
    });
    cy.mount(<App />);
    waitForDaemonSessionShell();
    cy.wait("@getAuthStatus");
    cy.get("[data-testid='terminal-route-unknown-session']", { timeout: 8000 }).should("be.visible");
    cy.get("[data-testid='terminal-route-unknown-session-home']").should("be.visible").click();
    cy.window().its("location.pathname").should("eq", "/");
    cy.get(`[data-testid="sessions-table-${PROJECT.projectId}"]`, { timeout: 8000 }).should("exist");
    cy.get("[data-testid='terminal-route-unknown-session']").should("not.exist");
  });
});
