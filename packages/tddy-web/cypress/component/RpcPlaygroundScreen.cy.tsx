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
import { byTestId, TEST_IDS } from "../support/testIds";
import { anAuthStatusAuthenticated, aGenerateTokenResponse } from "../support/rpc/responses";
import { toArrayBuffer } from "../support/rpc/protoRpc";

// These imports fail until the screen and route helpers are created.
import { RpcPlaygroundScreen } from "../../src/rpc-playground/RpcPlaygroundScreen";
import {
  RPC_PLAYGROUND_ROUTE,
  isRpcPlaygroundPath,
} from "../../src/routing/appRoutes";

// ---------------------------------------------------------------------------
// Test fixtures
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("RpcPlaygroundScreen — component (Cypress)", () => {
  beforeEach(() => {
    cy.intercept("POST", "**/auth.AuthService/GetAuthStatus", {
      statusCode: 200,
      body: toArrayBuffer(anAuthStatusAuthenticated()),
      headers: { "content-type": "application/proto" },
    });
    cy.intercept("POST", "**/token.TokenService/GenerateToken", {
      statusCode: 200,
      body: toArrayBuffer(aGenerateTokenResponse()),
      headers: { "content-type": "application/proto" },
    });
  });

  it("renders participant picker and service/method tree from mocked reflection", () => {
    // Given / When
    cy.mount(
      <RpcPlaygroundScreen
        services={MOCK_SERVICES}
        onInvoke={() => Promise.resolve({ kind: "success" as const, json: "{}" })}
        onNavigate={() => {}}
      />
    );

    // Then
    byTestId(TEST_IDS.rpcPlaygroundParticipantSelect).should("exist");
    byTestId(TEST_IDS.rpcServiceTree).should("exist");
    cy.contains("test.EchoService").should("be.visible");
  });

  it("expanding a service shows its methods with kind badges", () => {
    // Given
    cy.mount(
      <RpcPlaygroundScreen
        services={MOCK_SERVICES}
        onInvoke={() => Promise.resolve({ kind: "success" as const, json: "{}" })}
        onNavigate={() => {}}
      />
    );

    // When
    cy.contains("test.EchoService").click();

    // Then
    cy.contains("Echo").should("be.visible");
    cy.contains("unary").should("be.visible");
    cy.contains("EchoServerStream").should("be.visible");
    cy.contains("server_streaming").should("be.visible");
  });

  it("selecting a method seeds the request editor with default JSON", () => {
    // Given
    cy.mount(
      <RpcPlaygroundScreen
        services={MOCK_SERVICES}
        onInvoke={() => Promise.resolve({ kind: "success" as const, json: "{}" })}
        onNavigate={() => {}}
      />
    );

    // When
    cy.contains("test.EchoService").click();
    cy.contains("Echo").click();

    // Then — default JSON for a method with no schema is the empty object
    byTestId(TEST_IDS.rpcRequestEditor).should("exist");
    byTestId(TEST_IDS.rpcRequestRawJson).should("contain", "{");
  });

  it("toggling builder view and raw view retains the request value", () => {
    // Given
    cy.mount(
      <RpcPlaygroundScreen
        services={MOCK_SERVICES}
        onInvoke={() => Promise.resolve({ kind: "success" as const, json: "{}" })}
        onNavigate={() => {}}
      />
    );
    cy.contains("test.EchoService").click();
    cy.contains("Echo").click();

    // When — switch to raw JSON, type a value, toggle back and forth
    byTestId(TEST_IDS.rpcEditorToggleRaw).click();
    byTestId(TEST_IDS.rpcRequestRawJson).clear().type('{"message":"retain-me"}');
    byTestId(TEST_IDS.rpcEditorToggleBuilder).click();
    byTestId(TEST_IDS.rpcEditorToggleRaw).click();

    // Then — value is preserved (single source of truth)
    byTestId(TEST_IDS.rpcRequestRawJson).should("contain", "retain-me");
  });

  it("clicking Invoke shows decoded response JSON", () => {
    // Given
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

    // When
    byTestId(TEST_IDS.rpcInvokeButton).click();

    // Then
    byTestId(TEST_IDS.rpcResponse).should("contain", "playground-echo");
  });

  it("shows error code and message when invocation fails", () => {
    // Given
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

    // When
    byTestId(TEST_IDS.rpcInvokeButton).click();

    // Then
    byTestId(TEST_IDS.rpcError).should("contain", "not_found");
    byTestId(TEST_IDS.rpcError).should("contain", "Unknown method");
  });
});

describe("DaemonNavMenu — RPC Playground entry", () => {
  it("renders shell-menu-rpc-playground menu item", () => {
    // Given
    const { DaemonNavMenu } = require("../../src/components/shell/DaemonNavMenu");
    cy.mount(<DaemonNavMenu onNavigate={cy.stub().as("onNavigate")} />);

    // When
    byTestId(TEST_IDS.shellMenuButton).click();

    // Then
    byTestId(TEST_IDS.shellMenuRpcPlayground).should("be.visible");
  });

  it("clicking shell-menu-rpc-playground navigates to /rpc-playground", () => {
    // Given
    const { DaemonNavMenu } = require("../../src/components/shell/DaemonNavMenu");
    cy.mount(<DaemonNavMenu onNavigate={cy.stub().as("onNavigate")} />);
    byTestId(TEST_IDS.shellMenuButton).click();

    // When
    byTestId(TEST_IDS.shellMenuRpcPlayground).click();

    // Then
    cy.get("@onNavigate").should("have.been.calledWith", "/rpc-playground");
  });
});
