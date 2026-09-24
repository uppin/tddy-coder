/**
 * Acceptance tests: the **redirect-flow** GitHub sign-in callback.
 *
 * The operator returns from GitHub with an authorization code, and the page trades it with
 * `ExchangeCode` for the session triple — a user, an access token and a refresh token. The page
 * takes up that session only when every part of it is there: an exchange missing any part ends
 * sign-in with an error naming what was missing, keeps nothing, and leaves the operator signed out.
 * This is the same rule an approved device login is held to (`DeviceLoginAcceptance.cy.tsx`); the
 * page never fills a missing part in with a default.
 *
 * PRD: docs/ft/desktop/1-WIP/PRD-2026-09-19-keyring-desktop-login.md
 */

import React, { useEffect } from "react";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AuthProvider, useAuthContext } from "../../src/hooks/authProvider";
import { AuthService } from "../../src/gen/auth_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { TEST_IDS } from "../support/testIds";
import { redirectLoginPage } from "../support/pages/redirectLoginPage";
import {
  aCodeExchange,
  aCodeExchangeMissing,
  aRedirectLoginBackend,
  ACCESS_TOKEN_KEY,
  EXCHANGED_ACCESS_TOKEN,
  EXCHANGED_REFRESH_TOKEN,
  GITHUB_AUTHORIZATION_CODE,
  OAUTH_STATE,
  REFRESH_TOKEN_KEY,
  type CompletedSessionPart,
} from "../support/rpc/redirectLoginBackend";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/** What the operator is told when a code exchange lacks one part of the session — `useAuth.ts`. */
const NO_WHOLE_SESSION_MESSAGES: Record<CompletedSessionPart, string> = {
  user: "The daemon exchanged the sign-in code without a whole session (no user)",
  sessionToken: "The daemon exchanged the sign-in code without a whole session (no session token)",
  refreshToken: "The daemon exchanged the sign-in code without a whole session (no refresh token)",
};

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/** Renders the shared auth context's view of the operator, and the error it holds. */
function AuthProbe() {
  const { isAuthenticated, user, error } = useAuthContext();
  return (
    <>
      <div data-testid={TEST_IDS.authProbeStatus}>
        {isAuthenticated ? `signed-in:${user?.login ?? ""}` : "signed-out"}
      </div>
      <div data-testid={TEST_IDS.authProbeError}>{error ?? ""}</div>
    </>
  );
}

/** Hands the code and state GitHub returned to the auth context, as `/auth/callback` does. */
function ReturnFromGitHub({ code, state }: { code: string; state: string }) {
  const { handleCallback } = useAuthContext();
  useEffect(() => {
    void handleCallback(code, state);
  }, [handleCallback, code, state]);
  return null;
}

/** A signed-out operator returning from GitHub with an authorization code. */
function whenTheOperatorReturnsFromGitHub(backend: InMemoryRpcBackend) {
  mountWithRpc(
    <AuthProvider>
      <ReturnFromGitHub code={GITHUB_AUTHORIZATION_CODE} state={OAUTH_STATE} />
      <AuthProbe />
    </AuthProvider>,
    backend,
  );
}

function givenASignedOutOperator() {
  cy.clearLocalStorage();
}

function expectTheCodeExchanged(backend: InMemoryRpcBackend) {
  cy.wrap(null).should(() => {
    const exchanges = backend.callsTo(AuthService.method.exchangeCode);
    expect(exchanges.map((exchange) => [exchange.code, exchange.state]), "ExchangeCode calls").to.deep.equal([
      [GITHUB_AUTHORIZATION_CODE, OAUTH_STATE],
    ]);
  });
}

function expectStoredSession(accessToken: string, refreshToken: string) {
  cy.window().should((win) => {
    expect(win.localStorage.getItem(ACCESS_TOKEN_KEY), "stored access token").to.equal(accessToken);
    expect(win.localStorage.getItem(REFRESH_TOKEN_KEY), "stored refresh token").to.equal(refreshToken);
  });
}

function expectNothingStored() {
  cy.window().should((win) => {
    expect(win.localStorage.getItem(ACCESS_TOKEN_KEY), "stored access token").to.equal(null);
    expect(win.localStorage.getItem(REFRESH_TOKEN_KEY), "stored refresh token").to.equal(null);
  });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("Redirect-flow GitHub sign-in callback", () => {
  it("signs the operator in and stores the session when the code exchange returns a whole session", () => {
    // Given
    givenASignedOutOperator();
    const backend = aRedirectLoginBackend(aCodeExchange());

    // When
    whenTheOperatorReturnsFromGitHub(backend);

    // Then
    expectTheCodeExchanged(backend);
    redirectLoginPage.expectSignedInAs("octocat");
    redirectLoginPage.expectNoSignInError();
    expectStoredSession(EXCHANGED_ACCESS_TOKEN, EXCHANGED_REFRESH_TOKEN);
  });

  const EVERY_PART_OF_A_SESSION: CompletedSessionPart[] = ["user", "sessionToken", "refreshToken"];

  EVERY_PART_OF_A_SESSION.forEach((missing) => {
    it(`ends sign-in with an error when the code exchange returns no whole session (no ${missing})`, () => {
      // Given — the daemon's answer to the code lacks one part of the session
      givenASignedOutOperator();
      const backend = aRedirectLoginBackend(aCodeExchangeMissing(missing));

      // When
      whenTheOperatorReturnsFromGitHub(backend);

      // Then — the operator is told what was missing, stays signed out, and nothing is kept
      expectTheCodeExchanged(backend);
      redirectLoginPage.expectSignInError(NO_WHOLE_SESSION_MESSAGES[missing]);
      redirectLoginPage.expectSignedOut();
      expectNothingStored();
    });
  });
});
