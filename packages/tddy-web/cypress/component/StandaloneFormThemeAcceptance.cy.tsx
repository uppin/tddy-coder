/**
 * Acceptance test: the standalone (non-daemon) connection/sign-in form shares the shadcn theme —
 * it uses theme-token classes instead of inline hardcoded hex colors.
 *
 * PRD: docs/ft/web/app-shell.md § Theme
 */

import React from "react";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AuthService } from "../../src/gen/auth_pb";
import { App } from "../../src/index";
import { AuthProvider } from "../../src/hooks/authProvider";
import { RpcTransportProvider } from "../../src/rpc/transportProvider";
import { byTestId, TEST_IDS } from "../support/testIds";

const SIGN_IN_HELP = "Sign in with GitHub to access the terminal.";

describe("Standalone form theme", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    // App gates on GET /api/config (a fetch, not an RPC) — force standalone mode.
    cy.intercept("GET", "**/api/config", {
      statusCode: 200,
      headers: { "Content-Type": "application/json" },
      body: { daemon_mode: false },
    }).as("apiConfig");
  });

  it("styles the standalone sign-in help text with theme tokens, not inline hex", () => {
    // Given — standalone mode, unauthenticated
    const backend = anInMemoryRpcBackend().implement(AuthService, {
      getAuthStatus: async () => ({ authenticated: false, user: undefined }),
    });
    const transport = backend.transport();

    // When
    cy.mount(
      <RpcTransportProvider httpTransport={transport} liveKitFactory={() => transport}>
        <AuthProvider>
          <App />
        </AuthProvider>
      </RpcTransportProvider>,
    );

    // Then — the standalone sign-in form is shown with themed (not inline-styled) help text
    byTestId(TEST_IDS.githubLoginButton, { timeout: 5000 }).should("exist");
    cy.contains("p", SIGN_IN_HELP).should("have.class", "text-muted-foreground");
    cy.contains("p", SIGN_IN_HELP).should("not.have.attr", "style");
  });
});
