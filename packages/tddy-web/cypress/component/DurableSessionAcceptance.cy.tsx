/**
 * Acceptance tests for the durable web session (refresh-token + RPC token gate).
 *
 * Reproduces the reported bug — a user whose device slept past the 5-minute access-token TTL is
 * logged out — and pins the target behavior from
 * `docs/dev/1-WIP/2026-07-04-durable-web-session.md`: with an expired access token but a valid
 * refresh token, the app silently mints a fresh access token (no login screen), the next RPC
 * carries that fresh token, concurrent callers trigger a single refresh, the top bar reflects the
 * in-flight refresh, and an expired refresh token ends the session.
 */

import React from "react";
import { AuthProvider, useAuthContext } from "../../src/hooks/authProvider";
import { useDaemonClient } from "../../src/rpc/selectedDaemon";
import { AuthService } from "../../src/gen/auth_pb";
import { ConnectionService } from "../../src/gen/connection_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import {
  aDurableSessionBackend,
  aDeferredRefreshSessionBackend,
  ACCESS_TOKEN_KEY,
  REFRESH_TOKEN_KEY,
  EXPIRED_ACCESS_TOKEN,
  VALID_REFRESH_TOKEN,
  EXPIRED_REFRESH_TOKEN,
  REFRESHED_ACCESS_TOKEN,
} from "../support/rpc/durableSessionBackend";

// ---------------------------------------------------------------------------
// Harness probes
// ---------------------------------------------------------------------------

const tokenTestId = (probeId: string) => `session-probe-token-${probeId}`;
const authTestId = (probeId: string) => `session-probe-auth-${probeId}`;
const refreshingTestId = "session-probe-refreshing";
const listSessionsBtnTestId = "session-probe-list-sessions";

/** Renders the shared access token and authentication flag from the durable session. */
function SessionProbe({ probeId }: { probeId: string }) {
  const { sessionToken, isAuthenticated } = useAuthContext();
  return (
    <div>
      <div data-testid={tokenTestId(probeId)}>{sessionToken ?? "none"}</div>
      <div data-testid={authTestId(probeId)}>{isAuthenticated ? "authenticated" : "logged-out"}</div>
    </div>
  );
}

/** Renders whether a session refresh is currently in flight — the state the top-bar indicator binds to. */
function RefreshingProbe() {
  const { isRefreshing } = useAuthContext() as { isRefreshing?: boolean };
  return <div data-testid={refreshingTestId}>{isRefreshing ? "refreshing" : "idle"}</div>;
}

/** Issues `ConnectionService.ListSessions` with the shared token — proves the fresh token reaches a daemon RPC. */
function DaemonRpcProbe() {
  const { sessionToken } = useAuthContext();
  const client = useDaemonClient(ConnectionService);
  return (
    <button
      data-testid={listSessionsBtnTestId}
      onClick={() => {
        void client?.listSessions({ sessionToken: sessionToken ?? "" });
      }}
    >
      list sessions
    </button>
  );
}

// ---------------------------------------------------------------------------
// Page object — raw selectors live here; test bodies read as behavior.
// ---------------------------------------------------------------------------

const session = {
  expectToken(probeId: string, value: string) {
    cy.get(`[data-testid='${tokenTestId(probeId)}']`, { timeout: 5000 }).should("have.text", value);
  },
  expectAuthenticated(probeId: string) {
    cy.get(`[data-testid='${authTestId(probeId)}']`, { timeout: 5000 }).should("have.text", "authenticated");
  },
  expectLoggedOut(probeId: string) {
    cy.get(`[data-testid='${authTestId(probeId)}']`, { timeout: 5000 }).should("have.text", "logged-out");
  },
  expectRefreshing() {
    cy.get(`[data-testid='${refreshingTestId}']`, { timeout: 5000 }).should("have.text", "refreshing");
  },
  expectNotRefreshing() {
    cy.get(`[data-testid='${refreshingTestId}']`, { timeout: 5000 }).should("have.text", "idle");
  },
  invokeListSessions() {
    cy.get(`[data-testid='${listSessionsBtnTestId}']`).click();
  },
  expectStorageCleared() {
    cy.window().should((win) => {
      expect(win.localStorage.getItem(ACCESS_TOKEN_KEY)).to.equal(null);
      expect(win.localStorage.getItem(REFRESH_TOKEN_KEY)).to.equal(null);
    });
  },
};

function givenStoredTokens(win: Window, accessToken: string, refreshToken: string) {
  win.localStorage.setItem(ACCESS_TOKEN_KEY, accessToken);
  win.localStorage.setItem(REFRESH_TOKEN_KEY, refreshToken);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("Durable web session — refresh-token + RPC token gate", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
  });

  it("keeps the user authenticated when the access token has expired but the refresh token is still valid", () => {
    // Given — a slept device: the stored access token is expired, the refresh token still valid
    cy.window().then((win) => givenStoredTokens(win, EXPIRED_ACCESS_TOKEN, VALID_REFRESH_TOKEN));

    // When — the app mounts with a backend that honours the refresh token
    mountWithRpc(
      <AuthProvider>
        <SessionProbe probeId="a" />
      </AuthProvider>,
      aDurableSessionBackend(),
    );

    // Then — the session survives: a fresh access token is minted, no drop to logged-out
    session.expectAuthenticated("a");
    session.expectToken("a", REFRESHED_ACCESS_TOKEN);
  });

  it("sends the freshly refreshed access token on a daemon RPC issued after waking with an expired token", () => {
    // Given — an expired access token and a valid refresh token
    const backend = aDurableSessionBackend();
    cy.window().then((win) => givenStoredTokens(win, EXPIRED_ACCESS_TOKEN, VALID_REFRESH_TOKEN));

    // When — the app mounts, refreshes, and then an RPC is issued with the shared token
    mountWithRecordingLiveKitRpc(
      <AuthProvider>
        {withSelectedDaemon(
          <div>
            <SessionProbe probeId="a" />
            <DaemonRpcProbe />
          </div>,
        )}
      </AuthProvider>,
      backend,
    );
    session.expectToken("a", REFRESHED_ACCESS_TOKEN);
    session.invokeListSessions();

    // Then — the daemon received the refreshed token, never the stale expired one
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(ConnectionService.method.listSessions);
      expect(calls.map((c) => c.sessionToken)).to.deep.equal([REFRESHED_ACCESS_TOKEN]);
    });
  });

  it("triggers exactly one RefreshSession when several consumers mount with an expired token", () => {
    // Given — an expired access token and a valid refresh token
    const backend = aDurableSessionBackend();
    cy.window().then((win) => givenStoredTokens(win, EXPIRED_ACCESS_TOKEN, VALID_REFRESH_TOKEN));

    // When — three consumers share one AuthProvider
    mountWithRpc(
      <AuthProvider>
        <SessionProbe probeId="a" />
        <SessionProbe probeId="b" />
        <SessionProbe probeId="c" />
      </AuthProvider>,
      backend,
    );

    // Then — every consumer converges on the refreshed token
    session.expectToken("a", REFRESHED_ACCESS_TOKEN);
    session.expectToken("b", REFRESHED_ACCESS_TOKEN);
    session.expectToken("c", REFRESHED_ACCESS_TOKEN);

    // And — the shared provider refreshed exactly once, not once per consumer
    cy.wrap(null).should(() => {
      expect(backend.callsTo(AuthService.method.refreshSession)).to.have.length(1);
    });
  });

  it("shows a refreshing indicator while a refresh is in flight and hides it once the token arrives", () => {
    // Given — an expired access token, a valid refresh token, and a refresh held open
    const { backend, releaseRefresh } = aDeferredRefreshSessionBackend();
    cy.window().then((win) => givenStoredTokens(win, EXPIRED_ACCESS_TOKEN, VALID_REFRESH_TOKEN));

    // When — the app mounts and begins refreshing
    mountWithRpc(
      <AuthProvider>
        <RefreshingProbe />
        <SessionProbe probeId="a" />
      </AuthProvider>,
      backend,
    );

    // Then — the top bar reflects the in-flight refresh
    session.expectRefreshing();

    // When — the refresh completes
    cy.then(() => releaseRefresh());

    // Then — the indicator clears and the fresh token is in effect
    session.expectNotRefreshing();
    session.expectToken("a", REFRESHED_ACCESS_TOKEN);
  });

  it("logs the user out when the refresh token has also expired", () => {
    // Given — both the access token and the refresh token have expired
    cy.window().then((win) => givenStoredTokens(win, EXPIRED_ACCESS_TOKEN, EXPIRED_REFRESH_TOKEN));

    // When — the app mounts
    mountWithRpc(
      <AuthProvider>
        <SessionProbe probeId="a" />
      </AuthProvider>,
      aDurableSessionBackend(),
    );

    // Then — the session ends: logged out and both stored tokens cleared
    session.expectLoggedOut("a");
    session.expectStorageCleared();
  });
});
