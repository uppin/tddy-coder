/**
 * RPC Playground — screen acceptance tests (presentational).
 *
 * Tests the RpcPlaygroundScreen component: participant picker, service/method tree,
 * request editor (builder ⇄ raw toggle), Invoke button, response view, error display.
 *
 * The screen is presentational: services and invocation arrive as props, so the specs mount it
 * with `mountWithClients` over an in-memory backend and need no live server. The shell's
 * RPC Playground menu entry is `tddy-web`'s and is covered there.
 */

import React from "react";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { rpcPlaygroundPage } from "../support/pages/rpcPlaygroundPage";
import { byTestId, TEST_IDS } from "../support/testIds";

import { RpcPlaygroundScreen } from "../../src/RpcPlaygroundScreen";

// ---------------------------------------------------------------------------
// Test fixtures
// ---------------------------------------------------------------------------

/**
 * Whether the connection in scope can serve a participant roster. These scenarios are about the
 * service tree and the request editor, so they state the ordinary case — a host reached over
 * LiveKit, where the picker is offered. The wire that carries no roster, and the explanation that
 * replaces the picker on it, are covered by `PresenceCapabilityGatingAcceptance.cy.tsx`.
 */
const A_CONNECTION_THAT_CARRIES_A_ROSTER = true;

// Mock reflection response: a fake service tree with EchoService.
const ECHO_SERVICE = "test.EchoService";

const MOCK_SERVICES = [
  {
    name: ECHO_SERVICE,
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
  it("renders participant picker and service/method tree from mocked reflection", () => {
    // Given / When
    cy.mountWithClients(
      <RpcPlaygroundScreen
        services={MOCK_SERVICES}
        onInvoke={() => Promise.resolve({ kind: "success" as const, json: "{}" })}
        presenceAvailable={A_CONNECTION_THAT_CARRIES_A_ROSTER}
        onNavigate={() => {}}
      />,
      anInMemoryRpcBackend(),
    );

    // Then
    byTestId(TEST_IDS.rpcPlaygroundParticipantSelect).should("exist");
    byTestId(TEST_IDS.rpcServiceTree).should("exist");
    cy.contains("test.EchoService").should("be.visible");
  });

  it("expanding a service shows its methods with kind badges", () => {
    // Given
    cy.mountWithClients(
      <RpcPlaygroundScreen
        services={MOCK_SERVICES}
        onInvoke={() => Promise.resolve({ kind: "success" as const, json: "{}" })}
        presenceAvailable={A_CONNECTION_THAT_CARRIES_A_ROSTER}
        onNavigate={() => {}}
      />,
      anInMemoryRpcBackend(),
    );

    // When
    rpcPlaygroundPage.expandService(ECHO_SERVICE);

    // Then
    cy.contains("Echo").should("be.visible");
    cy.contains("unary").should("be.visible");
    cy.contains("EchoServerStream").should("be.visible");
    // KIND_LABELS formats "server_streaming" as "server stream" for display.
    cy.contains("server stream").should("be.visible");
  });

  it("selecting a method seeds the request editor with default JSON", () => {
    // Given
    cy.mountWithClients(
      <RpcPlaygroundScreen
        services={MOCK_SERVICES}
        onInvoke={() => Promise.resolve({ kind: "success" as const, json: "{}" })}
        presenceAvailable={A_CONNECTION_THAT_CARRIES_A_ROSTER}
        onNavigate={() => {}}
      />,
      anInMemoryRpcBackend(),
    );

    // When
    rpcPlaygroundPage.expandService(ECHO_SERVICE);
    rpcPlaygroundPage.chooseMethod(ECHO_SERVICE, "Echo");

    // Then — default JSON for a method with no schema is the empty object
    byTestId(TEST_IDS.rpcRequestEditor).should("exist");
    byTestId(TEST_IDS.rpcRequestRawJson).should("contain", "{");
  });

  it("toggling builder view and raw view retains the request value", () => {
    // Given
    cy.mountWithClients(
      <RpcPlaygroundScreen
        services={MOCK_SERVICES}
        onInvoke={() => Promise.resolve({ kind: "success" as const, json: "{}" })}
        presenceAvailable={A_CONNECTION_THAT_CARRIES_A_ROSTER}
        onNavigate={() => {}}
      />,
      anInMemoryRpcBackend(),
    );
    rpcPlaygroundPage.expandService(ECHO_SERVICE);
    rpcPlaygroundPage.chooseMethod(ECHO_SERVICE, "Echo");

    // When — switch to raw JSON, type a value, toggle back and forth
    byTestId(TEST_IDS.rpcEditorToggleRaw).click();
    // parseSpecialCharSequences: false prevents Cypress from interpreting '{' as a special key.
    byTestId(TEST_IDS.rpcRequestRawJson).clear().type('{"message":"retain-me"}', { parseSpecialCharSequences: false });
    byTestId(TEST_IDS.rpcEditorToggleBuilder).click();
    byTestId(TEST_IDS.rpcEditorToggleRaw).click();

    // Then — value is preserved (single source of truth)
    byTestId(TEST_IDS.rpcRequestRawJson).should("contain", "retain-me");
  });

  it("clicking Invoke shows decoded response JSON", () => {
    // Given
    const responseJson = '{"message":"playground-echo","timestamp":"0"}';
    cy.mountWithClients(
      <RpcPlaygroundScreen
        services={MOCK_SERVICES}
        onInvoke={() => Promise.resolve({ kind: "success" as const, json: responseJson })}
        presenceAvailable={A_CONNECTION_THAT_CARRIES_A_ROSTER}
        onNavigate={() => {}}
      />,
      anInMemoryRpcBackend(),
    );
    rpcPlaygroundPage.expandService(ECHO_SERVICE);
    rpcPlaygroundPage.chooseMethod(ECHO_SERVICE, "Echo");

    // When
    byTestId(TEST_IDS.rpcInvokeButton).click();

    // Then
    byTestId(TEST_IDS.rpcResponse).should("contain", "playground-echo");
  });

  it("shows error code and message when invocation fails", () => {
    // Given
    cy.mountWithClients(
      <RpcPlaygroundScreen
        services={MOCK_SERVICES}
        onInvoke={() =>
          Promise.resolve({ kind: "error" as const, code: "not_found", message: "Unknown method" })
        }
        presenceAvailable={A_CONNECTION_THAT_CARRIES_A_ROSTER}
        onNavigate={() => {}}
      />,
      anInMemoryRpcBackend(),
    );
    rpcPlaygroundPage.expandService(ECHO_SERVICE);
    rpcPlaygroundPage.chooseMethod(ECHO_SERVICE, "Echo");

    // When
    byTestId(TEST_IDS.rpcInvokeButton).click();

    // Then
    byTestId(TEST_IDS.rpcError).should("contain", "not_found");
    byTestId(TEST_IDS.rpcError).should("contain", "Unknown method");
  });
});
