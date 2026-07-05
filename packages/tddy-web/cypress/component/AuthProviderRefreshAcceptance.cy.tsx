/**
 * Acceptance tests for the shared `AuthProvider` / `useAuthContext` session-token lifecycle.
 *
 * Reproduces the bug behind this provider: `useAuth()` used to be mounted independently in many
 * places (`App`, `ConnectionForm`, `SelectedDaemonProvider`, `WorktreesAppPage`, `VmsAppPage`,
 * `RpcPlaygroundAppPage`, `AuthCallback`) plus screens reading `localStorage` directly, so each
 * had its own session state and could refresh in an uncoordinated way; a daemon-switch remount
 * (`SelectedDaemonProvider` keys its children on `selectedInstanceId`) destroyed whichever instance
 * lived in that subtree.
 *
 * `AuthProvider` fixes this by owning the *only* session store. Every consumer reads the same value
 * via `useAuthContext()`, so a descendant remount cannot reset or duplicate anything, and the token
 * used for RPC calls to *any* connected daemon is the one shared, centrally refreshed value. Under
 * the two-token model, an expired access token is transparently re-minted from the refresh token
 * exactly once for the whole subtree rather than once per consumer.
 */

import React, { useState } from "react";
import { AuthProvider, useAuthContext } from "../../src/hooks/authProvider";
import { useDaemonClient } from "../../src/rpc/selectedDaemon";
import { AuthService } from "../../src/gen/auth_pb";
import { ConnectionService } from "../../src/gen/connection_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import {
  anAuthRefreshBackend,
  CURRENT_ACCESS_TOKEN,
  EXPIRED_ACCESS_TOKEN,
  VALID_REFRESH_TOKEN,
  REFRESHED_ACCESS_TOKEN,
  ACCESS_TOKEN_KEY,
  REFRESH_TOKEN_KEY,
} from "../support/rpc/authRefreshBackend";

// ---------------------------------------------------------------------------
// Test harness probes
// ---------------------------------------------------------------------------

const tokenTestId = (probeId: string) => `auth-probe-token-${probeId}`;
const remountBtnTestId = (probeId: string) => `auth-probe-remount-${probeId}`;
const listSessionsBtnTestId = (probeId: string) => `auth-probe-list-sessions-${probeId}`;

/** Renders the shared session token — mount several to prove every consumer agrees. */
function TokenProbe({ probeId }: { probeId: string }) {
  const { sessionToken } = useAuthContext();
  return <div data-testid={tokenTestId(probeId)}>{sessionToken ?? "none"}</div>;
}

/**
 * Wraps a `TokenProbe` in a remountable boundary — clicking the button remounts it via a `key`
 * change, simulating `SelectedDaemonProvider` keying its children on `selectedInstanceId` when the
 * operator switches to a different daemon.
 */
function RemountableTokenProbe({ probeId }: { probeId: string }) {
  const [generation, setGeneration] = useState(0);
  return (
    <div>
      <button data-testid={remountBtnTestId(probeId)} onClick={() => setGeneration((g) => g + 1)}>
        remount
      </button>
      <TokenProbe key={generation} probeId={probeId} />
    </div>
  );
}

/**
 * Issues `ConnectionService.ListSessions` (daemon-level RPC) with the shared session token on
 * click — proves the token from `useAuthContext()` actually reaches a request sent to a connected
 * daemon, not just a display value.
 */
function DaemonRpcProbe({ probeId }: { probeId: string }) {
  const { sessionToken } = useAuthContext();
  const client = useDaemonClient(ConnectionService);
  return (
    <button
      data-testid={listSessionsBtnTestId(probeId)}
      onClick={() => {
        void client?.listSessions({ sessionToken: sessionToken ?? "" });
      }}
    >
      list sessions
    </button>
  );
}

// ---------------------------------------------------------------------------
// Page object — all raw selectors live here, test bodies call named methods.
// ---------------------------------------------------------------------------

const authProbe = {
  token: (probeId: string) => cy.get(`[data-testid='${tokenTestId(probeId)}']`, { timeout: 5000 }),

  expectToken(probeId: string, value: string) {
    authProbe.token(probeId).should("have.text", value);
  },

  remount(probeId: string) {
    cy.get(`[data-testid='${remountBtnTestId(probeId)}']`).click();
  },

  invokeListSessions(probeId: string) {
    cy.get(`[data-testid='${listSessionsBtnTestId(probeId)}']`).click();
  },
};

function givenStoredTokens(win: Window, accessToken: string, refreshToken: string) {
  win.localStorage.setItem(ACCESS_TOKEN_KEY, accessToken);
  win.localStorage.setItem(REFRESH_TOKEN_KEY, refreshToken);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("AuthProvider — shared session-token lifecycle", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
  });

  it("shares one session token across every consumer in its subtree", () => {
    // Given — a currently-valid access token that needs no refresh
    cy.window().then((win) => givenStoredTokens(win, CURRENT_ACCESS_TOKEN, VALID_REFRESH_TOKEN));
    const backend = anAuthRefreshBackend();

    // When — two independent components consume the shared context
    mountWithRpc(
      <AuthProvider>
        <TokenProbe probeId="a" />
        <TokenProbe probeId="b" />
      </AuthProvider>,
      backend,
    );

    // Then — both see the same token, and no refresh was needed
    authProbe.expectToken("a", CURRENT_ACCESS_TOKEN);
    authProbe.expectToken("b", CURRENT_ACCESS_TOKEN);
    cy.wrap(null).should(() => {
      expect(backend.callsTo(AuthService.method.refreshSession)).to.have.length(0);
    });
  });

  it("re-mints the shared token exactly once for every consumer when the access token has expired", () => {
    // Given — three consumers and a slept-device state (expired access, valid refresh)
    cy.window().then((win) => givenStoredTokens(win, EXPIRED_ACCESS_TOKEN, VALID_REFRESH_TOKEN));
    const backend = anAuthRefreshBackend();
    mountWithRpc(
      <AuthProvider>
        <TokenProbe probeId="a" />
        <TokenProbe probeId="b" />
        <TokenProbe probeId="c" />
      </AuthProvider>,
      backend,
    );

    // Then — exactly one RefreshSession call was made, not one per consumer
    cy.wrap(null).should(() => {
      expect(backend.callsTo(AuthService.method.refreshSession)).to.have.length(1);
    });

    // And — every consumer observes the same refreshed token
    authProbe.expectToken("a", REFRESHED_ACCESS_TOKEN);
    authProbe.expectToken("b", REFRESHED_ACCESS_TOKEN);
    authProbe.expectToken("c", REFRESHED_ACCESS_TOKEN);
  });

  it("keeps a remounted descendant on the current token without a second refresh", () => {
    // Given — a consumer inside a boundary that gets remounted independently, as
    // SelectedDaemonProvider does to its children on every daemon switch
    cy.window().then((win) => givenStoredTokens(win, EXPIRED_ACCESS_TOKEN, VALID_REFRESH_TOKEN));
    const backend = anAuthRefreshBackend();
    mountWithRpc(
      <AuthProvider>
        <RemountableTokenProbe probeId="a" />
      </AuthProvider>,
      backend,
    );
    authProbe.expectToken("a", REFRESHED_ACCESS_TOKEN);

    // When — the descendant remounts (e.g. a daemon switch)
    authProbe.remount("a");

    // Then — the freshly remounted consumer immediately shows the current token
    authProbe.expectToken("a", REFRESHED_ACCESS_TOKEN);

    // And — the remount did not trigger a second refresh
    cy.wrap(null).should(() => {
      expect(backend.callsTo(AuthService.method.refreshSession)).to.have.length(1);
    });
  });

  it("carries the refreshed token into a daemon-level RPC call made through useDaemonClient", () => {
    // Given — a slept-device state routed over the shared common-room LiveKit connection
    cy.window().then((win) => givenStoredTokens(win, EXPIRED_ACCESS_TOKEN, VALID_REFRESH_TOKEN));
    const backend = anAuthRefreshBackend();
    mountWithRecordingLiveKitRpc(
      <AuthProvider>
        {withSelectedDaemon(
          <div>
            <TokenProbe probeId="a" />
            <DaemonRpcProbe probeId="a" />
          </div>,
        )}
      </AuthProvider>,
      backend,
    );

    // When — the shared token has refreshed and the operator triggers a call
    authProbe.expectToken("a", REFRESHED_ACCESS_TOKEN);
    authProbe.invokeListSessions("a");

    // Then — the daemon received the refreshed token, not a stale one
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(ConnectionService.method.listSessions);
      expect(calls.map((c) => c.sessionToken)).to.deep.equal([REFRESHED_ACCESS_TOKEN]);
    });
  });
});
