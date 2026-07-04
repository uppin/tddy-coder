/**
 * Acceptance tests for the shared `AuthProvider` / `useAuthContext` session-token lifecycle.
 *
 * Reproduces the reported bug: RPC calls fail with an expired token because `useAuth()` is
 * currently mounted independently in up to 7 places (`App`, `ConnectionForm`,
 * `SelectedDaemonProvider`, `WorktreesAppPage`, `VmsAppPage`, `RpcPlaygroundAppPage`,
 * `AuthCallback`), each with its own React state and its own 4-minute refresh timer — plus three
 * more screens (`SessionsDrawerScreen`, `TasksDrawerScreen`, `ProjectsAppPage`) that read
 * `localStorage` directly and never refresh anything themselves. `SelectedDaemonProvider` keys its
 * children on `selectedInstanceId` (`src/rpc/selectedDaemon.tsx`), so switching to a non-default
 * daemon remounts whichever of those independent `useAuth()` instances happened to live in that
 * subtree, destroying its timer.
 *
 * `AuthProvider` fixes this by being the *only* place that owns the session token and its refresh
 * timer. Every consumer reads the same value via `useAuthContext()`, so a descendant remount (a
 * daemon switch) cannot reset or duplicate the timer, and the token used for RPC calls to *any*
 * connected daemon is always the one shared, centrally refreshed value.
 */

import React, { useState } from "react";
import { AuthProvider, useAuthContext } from "../../src/hooks/authProvider";
import { useDaemonClient } from "../../src/rpc/selectedDaemon";
import { AuthService } from "../../src/gen/auth_pb";
import { ConnectionService } from "../../src/gen/connection_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { anAuthRefreshBackend } from "../support/rpc/authRefreshBackend";

// Matches the (currently unexported) `SESSION_REFRESH_INTERVAL_MS` in `src/hooks/useAuth.ts` —
// `AuthProvider` reuses that same interval for its single, centralized refresh timer.
const SESSION_REFRESH_INTERVAL_MS = 4 * 60 * 1000;

const INITIAL_TOKEN = "session-token-v1";

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
 * click — proves the refreshed token from `useAuthContext()` actually reaches a request sent to a
 * connected daemon, not just a display value.
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("AuthProvider — shared session-token lifecycle", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.window().then((win) => win.localStorage.setItem("tddy_session_token", INITIAL_TOKEN));
  });

  it("shares one session token across every consumer in its subtree", () => {
    // Given — a backend that authenticates the stored token
    const backend = anAuthRefreshBackend();

    // When — two independent components consume the shared context
    mountWithRpc(
      <AuthProvider>
        <TokenProbe probeId="a" />
        <TokenProbe probeId="b" />
      </AuthProvider>,
      backend,
    );

    // Then — both see the same token
    authProbe.expectToken("a", INITIAL_TOKEN);
    authProbe.expectToken("b", INITIAL_TOKEN);
  });

  it("refreshes the shared token exactly once per interval and updates every consumer", () => {
    // Given — three consumers, mirroring today's App / WorktreesAppPage / VmsAppPage each
    // mounting their own independent useAuth() instance
    const backend = anAuthRefreshBackend();
    cy.clock();
    mountWithRpc(
      <AuthProvider>
        <TokenProbe probeId="a" />
        <TokenProbe probeId="b" />
        <TokenProbe probeId="c" />
      </AuthProvider>,
      backend,
    );
    authProbe.expectToken("a", INITIAL_TOKEN);

    // When — the refresh interval elapses
    cy.tick(SESSION_REFRESH_INTERVAL_MS);

    // Then — exactly one RefreshSession call was made, not one per consumer
    cy.wrap(null).should(() => {
      expect(backend.callsTo(AuthService.method.refreshSession)).to.have.length(1);
    });

    // And — every consumer observes the same refreshed token
    const refreshedToken = `${INITIAL_TOKEN}-refreshed`;
    authProbe.expectToken("a", refreshedToken);
    authProbe.expectToken("b", refreshedToken);
    authProbe.expectToken("c", refreshedToken);
  });

  it("keeps refreshing on schedule after a descendant subtree remounts", () => {
    // Given — a consumer inside a boundary that gets remounted independently, as
    // SelectedDaemonProvider does to its children on every daemon switch
    const backend = anAuthRefreshBackend();
    cy.clock();
    mountWithRpc(
      <AuthProvider>
        <RemountableTokenProbe probeId="a" />
      </AuthProvider>,
      backend,
    );
    authProbe.expectToken("a", INITIAL_TOKEN);

    // When — the descendant remounts (e.g. a daemon switch)
    authProbe.remount("a");

    // Then — the freshly remounted consumer immediately shows the current token, with no reset to
    // a loading/empty state
    authProbe.expectToken("a", INITIAL_TOKEN);

    // When — the refresh interval elapses afterwards
    cy.tick(SESSION_REFRESH_INTERVAL_MS);

    // Then — the remount did not duplicate the timer: still exactly one RefreshSession call, and
    // the remounted consumer picks up the refreshed token
    cy.wrap(null).should(() => {
      expect(backend.callsTo(AuthService.method.refreshSession)).to.have.length(1);
    });
    authProbe.expectToken("a", `${INITIAL_TOKEN}-refreshed`);
  });

  it("carries the refreshed token into a daemon-level RPC call made through useDaemonClient", () => {
    // Given — a probe that issues ConnectionService.ListSessions with the shared token, routed
    // over the shared common-room LiveKit connection to a selected daemon
    const backend = anAuthRefreshBackend();
    cy.clock();
    mountWithRecordingLiveKitRpc(
      <AuthProvider>{withSelectedDaemon(<DaemonRpcProbe probeId="a" />)}</AuthProvider>,
      backend,
    );

    // When — the operator triggers a call before any refresh has happened
    authProbe.invokeListSessions("a");

    // Then — the daemon received the original token
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(ConnectionService.method.listSessions);
      expect(calls.map((c) => c.sessionToken)).to.deep.equal([INITIAL_TOKEN]);
    });

    // When — the shared token is refreshed and the operator calls again
    cy.tick(SESSION_REFRESH_INTERVAL_MS);
    authProbe.invokeListSessions("a");

    // Then — the daemon received the refreshed token, not the stale pre-refresh one
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(ConnectionService.method.listSessions);
      expect(calls.map((c) => c.sessionToken)).to.deep.equal([INITIAL_TOKEN, `${INITIAL_TOKEN}-refreshed`]);
    });
  });
});
