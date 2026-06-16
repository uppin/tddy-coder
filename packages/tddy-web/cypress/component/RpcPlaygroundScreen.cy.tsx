/**
 * RPC Playground — screen acceptance tests (presentational, intercept-driven).
 *
 * Tests the RpcPlaygroundScreen component: participant picker, service/method tree,
 * request editor (builder ⇄ raw toggle), Invoke button, response view, error display.
 *
 * Uses cy.intercept to mock RPC calls over HTTP (token, auth, reflection) so these
 * tests do not require a live LiveKit server.
 *
 * All imports will FAIL until:
 *   1. RpcPlaygroundScreen.tsx is created in packages/tddy-web/src/rpc-playground/
 *   2. The routing helpers (isRpcPlaygroundPath, RPC_PLAYGROUND_ROUTE) are added to appRoutes.ts
 *   3. DaemonNavMenu includes the rpc-playground menu item
 *
 * ⚠️ RED PHASE — these tests are intentionally failing.
 */

import React from "react";
import { create, toBinary } from "@bufbuild/protobuf";
import { GenerateTokenResponseSchema } from "../../src/gen/token_pb";
import { GetAuthStatusResponseSchema, GitHubUserSchema } from "../../src/gen/auth_pb";

// These imports fail until the screen and route helpers are created.
import { RpcPlaygroundScreen } from "../../src/rpc-playground/RpcPlaygroundScreen";
import {
  RPC_PLAYGROUND_ROUTE,
  isRpcPlaygroundPath,
} from "../../src/routing/appRoutes";

const toArrayBuffer = (u8: Uint8Array): ArrayBuffer => {
  const buf = new ArrayBuffer(u8.length);
  new Uint8Array(buf).set(u8);
  return buf;
};

// ---------------------------------------------------------------------------
// Mock helpers
// ---------------------------------------------------------------------------

function mockTokenResponse() {
  return toArrayBuffer(
    toBinary(
      GenerateTokenResponseSchema,
      create(GenerateTokenResponseSchema, { token: "mock-token", ttlSeconds: 300 }),
    ),
  );
}

function mockAuthAuthenticated() {
  const user = create(GitHubUserSchema, {
    login: "testuser",
    avatarUrl: "https://example.com/avatar.png",
    name: "Test User",
    id: BigInt(42),
  });
  return toArrayBuffer(
    toBinary(
      GetAuthStatusResponseSchema,
      create(GetAuthStatusResponseSchema, { authenticated: true, user }),
    ),
  );
}

// Mock reflection response: a fake service tree with EchoService.
const MOCK_SERVICES = [
  {
    name: "test.EchoService",
    methods: [
      { name: "Echo", kind: "unary" as const },
      { name: "EchoServerStream", kind: "server_streaming" as const },
      { name: "EchoClientStream", kind: "client_streaming" as const },
      { name: "EchoBidiStream", kind: "bidi_streaming" as const },
    ],
  },
];

// Mock transport for dynamic invocation (passed as prop).
const mockTransport = {
  services: MOCK_SERVICES,
  lastInvokedService: null as string | null,
  lastInvokedMethod: null as string | null,
  mockResponseJson: '{"message":"playground-echo","timestamp":"0"}',
};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("RpcPlaygroundScreen — component (Cypress)", () => {
  beforeEach(() => {
    cy.intercept("POST", "**/auth.AuthService/GetAuthStatus", {
      statusCode: 200,
      body: mockAuthAuthenticated(),
      headers: { "content-type": "application/proto" },
    });
    cy.intercept("POST", "**/token.TokenService/GenerateToken", {
      statusCode: 200,
      body: mockTokenResponse(),
      headers: { "content-type": "application/proto" },
    });
  });

  it("renders participant picker and service/method tree from mocked reflection", () => {
    cy.mount(
      <RpcPlaygroundScreen
        services={MOCK_SERVICES}
        onInvoke={() => Promise.resolve({ kind: "success" as const, json: "{}" })}
        onNavigate={() => {}}
      />
    );

    cy.get('[data-testid="rpc-playground-participant-select"]').should("exist");
    cy.get('[data-testid="rpc-service-tree"]').should("exist");
    cy.contains("test.EchoService").should("be.visible");
  });

  it("expanding a service shows its methods with kind badges", () => {
    cy.mount(
      <RpcPlaygroundScreen
        services={MOCK_SERVICES}
        onInvoke={() => Promise.resolve({ kind: "success" as const, json: "{}" })}
        onNavigate={() => {}}
      />
    );

    cy.contains("test.EchoService").click();
    cy.contains("Echo").should("be.visible");
    cy.contains("unary").should("be.visible");
    cy.contains("EchoServerStream").should("be.visible");
    cy.contains("server_streaming").should("be.visible");
  });

  it("selecting a method seeds the request editor with default JSON", () => {
    cy.mount(
      <RpcPlaygroundScreen
        services={MOCK_SERVICES}
        onInvoke={() => Promise.resolve({ kind: "success" as const, json: "{}" })}
        onNavigate={() => {}}
      />
    );

    cy.contains("test.EchoService").click();
    cy.contains("Echo").click();
    cy.get('[data-testid="rpc-request-editor"]').should("exist");
    // Default JSON for a method with no schema is the empty object.
    cy.get('[data-testid="rpc-request-raw-json"]').should("contain", "{");
  });

  it("toggling builder view and raw view retains the request value", () => {
    cy.mount(
      <RpcPlaygroundScreen
        services={MOCK_SERVICES}
        onInvoke={() => Promise.resolve({ kind: "success" as const, json: "{}" })}
        onNavigate={() => {}}
      />
    );

    cy.contains("test.EchoService").click();
    cy.contains("Echo").click();

    // Switch to raw JSON view and type a value.
    cy.get('[data-testid="rpc-editor-toggle-raw"]').click();
    cy.get('[data-testid="rpc-request-raw-json"]')
      .clear()
      .type('{"message":"retain-me"}');

    // Switch to builder view and back.
    cy.get('[data-testid="rpc-editor-toggle-builder"]').click();
    cy.get('[data-testid="rpc-editor-toggle-raw"]').click();

    // Value must still be present (single source of truth).
    cy.get('[data-testid="rpc-request-raw-json"]').should("contain", "retain-me");
  });

  it("clicking Invoke shows decoded response JSON", () => {
    const responseJson = '{"message":"playground-echo","timestamp":"0"}';
    cy.mount(
      <RpcPlaygroundScreen
        services={MOCK_SERVICES}
        onInvoke={() => Promise.resolve({ kind: "success" as const, json: responseJson })}
        onNavigate={() => {}}
      />
    );

    cy.contains("test.EchoService").click();
    cy.contains("Echo").click();
    cy.get('[data-testid="rpc-invoke-button"]').click();
    cy.get('[data-testid="rpc-response"]').should("contain", "playground-echo");
  });

  it("shows error code and message when invocation fails", () => {
    cy.mount(
      <RpcPlaygroundScreen
        services={MOCK_SERVICES}
        onInvoke={() =>
          Promise.resolve({ kind: "error" as const, code: "not_found", message: "Unknown method" })
        }
        onNavigate={() => {}}
      />
    );

    cy.contains("test.EchoService").click();
    cy.contains("Echo").click();
    cy.get('[data-testid="rpc-invoke-button"]').click();
    cy.get('[data-testid="rpc-error"]').should("contain", "not_found");
    cy.get('[data-testid="rpc-error"]').should("contain", "Unknown method");
  });
});

describe("DaemonNavMenu — RPC Playground entry", () => {
  it("renders shell-menu-rpc-playground menu item", () => {
    // Import DaemonNavMenu which must have the new menu item.
    const { DaemonNavMenu } = require("../../src/components/shell/DaemonNavMenu");

    cy.mount(
      <DaemonNavMenu onNavigate={cy.stub().as("onNavigate")} />
    );

    cy.get('[data-testid="shell-menu-button"]').click();
    cy.get('[data-testid="shell-menu-rpc-playground"]').should("be.visible");
  });

  it("clicking shell-menu-rpc-playground navigates to /rpc-playground", () => {
    const { DaemonNavMenu } = require("../../src/components/shell/DaemonNavMenu");

    cy.mount(
      <DaemonNavMenu onNavigate={cy.stub().as("onNavigate")} />
    );

    cy.get('[data-testid="shell-menu-button"]').click();
    cy.get('[data-testid="shell-menu-rpc-playground"]').click();
    cy.get("@onNavigate").should("have.been.calledWith", "/rpc-playground");
  });
});
