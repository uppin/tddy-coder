/**
 * Acceptance tests: the **device-flow** GitHub sign-in screen.
 *
 * A deployment configured with a public `client_id` and no client secret — Tddy Desktop — cannot
 * run the redirect flow (`GetAuthUrl` / `ExchangeCode`): it has no callback URL and no secret to
 * exchange a code with. It serves `auth.StartDeviceLogin` / `auth.PollDeviceLogin` instead. The
 * operator is shown a short user code and a link to GitHub's verification page; the page polls no
 * faster than the interval it was given until GitHub says the code was approved, refused, or
 * expired. An approval yields the same session triple `ExchangeCode` returns, so the operator ends
 * up signed in exactly as the redirect flow would leave them.
 *
 * Which of the two flows a sign-in screen offers is the daemon's to declare: `/api/config` carries
 * `auth_flow` (`"device"` or `"redirect"`), and a daemon that does not name one predates the device
 * flow and serves only the redirect flow.
 *
 * Every poll-timing test runs under `cy.clock()`, so "waits the interval" is proven by the clock,
 * not by a real-time sleep.
 *
 * PRD: docs/ft/desktop/1-WIP/PRD-2026-09-19-keyring-desktop-login.md
 * Changeset: docs/dev/1-WIP/2026-09-19-keyring-desktop-login.md (M7)
 */

import React from "react";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AuthProvider, useAuthContext } from "../../src/hooks/authProvider";
import { AuthService } from "../../src/gen/auth_pb";
import { DeviceLoginPanel } from "../../src/components/DeviceLoginPanel";
import { DaemonLoginScreen } from "../../src/components/DaemonLoginScreen";
import { loadClientConfig } from "../../src/rpc/clientConfig";
import { mountWithRpc } from "../support/rpc/inMemory";
import { TEST_IDS } from "../support/testIds";
import { deviceLoginPage } from "../support/pages/deviceLoginPage";
import {
  aCompletedPoll,
  aDeniedPoll,
  aDeviceCodeGrant,
  aDeviceLoginBackend,
  anExpiredPoll,
  aPendingPoll,
  aSlowDownPoll,
  ACCESS_TOKEN_KEY,
  DEVICE_LOGIN_ACCESS_TOKEN,
  DEVICE_LOGIN_REFRESH_TOKEN,
  GITHUB_DEVICE_VERIFICATION_URI,
  REFRESH_TOKEN_KEY,
} from "../support/rpc/deviceLoginBackend";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/** GitHub's default minimum gap between polls. */
const FIVE_SECOND_INTERVAL = 5;
/** The wider gap a slow-down answer tells the client to adopt. */
const TEN_SECOND_INTERVAL = 10;

const FIRST_GRANT = aDeviceCodeGrant({
  deviceCode: "3584d83530557fdd1f46af8289938c8ef79f9dc5",
  userCode: "WDJB-MJHT",
  intervalSeconds: FIVE_SECOND_INTERVAL,
});

const SECOND_GRANT = aDeviceCodeGrant({
  deviceCode: "7c3f1e0a9b2d4e6f8a1c3e5f7a9b1d3f5e7a9c1e",
  userCode: "QKRT-4XZN",
  intervalSeconds: FIVE_SECOND_INTERVAL,
});

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/** Renders the shared auth context's view of the operator beside the panel under test. */
function AuthStatusProbe() {
  const { isAuthenticated, user } = useAuthContext();
  return (
    <div data-testid={TEST_IDS.authProbeStatus}>
      {isAuthenticated ? `signed-in:${user?.login ?? ""}` : "signed-out"}
    </div>
  );
}

/** A signed-out operator looking at the device-flow panel, with the page's clock frozen. */
function givenTheDeviceSignInPanel(backend: InMemoryRpcBackend) {
  cy.clearLocalStorage();
  cy.clock();
  mountWithRpc(
    <AuthProvider>
      <DeviceLoginPanel />
      <AuthStatusProbe />
    </AuthProvider>,
    backend,
  );
}

/**
 * Let the answer to the RPC in flight land before the page's clock moves.
 *
 * A poll is recorded the moment it is *sent*; the client handles its answer — and schedules the
 * next poll — a few promise hops later. Cypress's command queue can advance between those hops, so
 * ticking straight after "one poll was sent" would schedule the next poll against a clock that has
 * already moved on, and prove nothing about the interval. One real (unfaked) macrotask boundary
 * flushes that chain; it does not wait for any time to pass on the page's frozen clock.
 */
function letTheAnswerInFlightLand() {
  cy.wait(0, { log: false });
}

/** Advance the page's frozen clock. */
function afterSeconds(seconds: number) {
  letTheAnswerInFlightLand();
  cy.tick(seconds * 1000);
}

/** Advance the page's frozen clock to one millisecond short of `seconds`. */
function justBeforeSeconds(seconds: number) {
  letTheAnswerInFlightLand();
  cy.tick(seconds * 1000 - 1);
}

/** Advance the page's frozen clock by one millisecond. */
function oneMillisecondLater() {
  letTheAnswerInFlightLand();
  cy.tick(1);
}

function pollsTo(backend: InMemoryRpcBackend) {
  return backend.callsTo(AuthService.method.pollDeviceLogin);
}

function expectPollCount(backend: InMemoryRpcBackend, count: number) {
  cy.wrap(null).should(() => {
    expect(pollsTo(backend), "PollDeviceLogin calls").to.have.length(count);
  });
}

function expectEveryPollPresented(backend: InMemoryRpcBackend, deviceCode: string, polls: number) {
  cy.wrap(null).should(() => {
    expect(pollsTo(backend).map((poll) => poll.deviceCode)).to.deep.equal(
      Array.from({ length: polls }, () => deviceCode),
    );
  });
}

function expectStartCount(backend: InMemoryRpcBackend, count: number) {
  cy.wrap(null).should(() => {
    expect(backend.callsTo(AuthService.method.startDeviceLogin), "StartDeviceLogin calls").to.have.length(
      count,
    );
  });
}

function expectNoRedirectFlowRequested(backend: InMemoryRpcBackend) {
  cy.wrap(null).should(() => {
    expect(backend.callsTo(AuthService.method.getAuthUrl), "GetAuthUrl calls").to.have.length(0);
    expect(backend.callsTo(AuthService.method.exchangeCode), "ExchangeCode calls").to.have.length(0);
  });
}

function expectStoredSession(accessToken: string, refreshToken: string) {
  cy.window().should((win) => {
    expect(win.localStorage.getItem(ACCESS_TOKEN_KEY), "stored access token").to.equal(accessToken);
    expect(win.localStorage.getItem(REFRESH_TOKEN_KEY), "stored refresh token").to.equal(refreshToken);
  });
}

/** The sign-in screen as `App` renders it for a daemon that declared `authFlow`. */
function givenTheSignInScreenFor(authFlow: "device" | "redirect" | undefined) {
  cy.clearLocalStorage();
  mountWithRpc(
    <AuthProvider>
      <DaemonLoginScreen path="/" login={cy.stub().as("redirectLogin")} authError={null} authFlow={authFlow} />
    </AuthProvider>,
    aDeviceLoginBackend({ polls: [aPendingPoll()] }),
  );
}

/** A daemon whose `/api/config` declares the sign-in flow it serves. */
function givenADaemonDeclaringAuthFlow(authFlow: "device" | "redirect") {
  cy.intercept("GET", "**/api/config", {
    statusCode: 200,
    body: { daemon_mode: true, daemon_instance_id: "desktop", auth_flow: authFlow },
  });
}

function whenThePageReadsItsClientConfig() {
  return cy.wrap(null).then(() => loadClientConfig(anInMemoryRpcBackend().transport()));
}

// ---------------------------------------------------------------------------
// Starting a device login
// ---------------------------------------------------------------------------

describe("Device-flow GitHub sign-in", () => {
  it("shows the user code and a link to GitHub's verification page once a device login starts", () => {
    // Given
    givenTheDeviceSignInPanel(aDeviceLoginBackend({ grants: [FIRST_GRANT], polls: [aPendingPoll()] }));

    // When
    deviceLoginPage.startSignIn();

    // Then
    deviceLoginPage.expectUserCode("WDJB-MJHT");
    deviceLoginPage.expectVerificationLinkTo(GITHUB_DEVICE_VERIFICATION_URI);
  });

  it("never asks the daemon for a redirect URL while signing in by device", () => {
    // Given
    const backend = aDeviceLoginBackend({ grants: [FIRST_GRANT], polls: [aPendingPoll()] });
    givenTheDeviceSignInPanel(backend);

    // When
    deviceLoginPage.startSignIn();
    deviceLoginPage.expectUserCode("WDJB-MJHT");
    afterSeconds(FIVE_SECOND_INTERVAL);

    // Then
    expectPollCount(backend, 1);
    expectNoRedirectFlowRequested(backend);
  });

  // -------------------------------------------------------------------------
  // Pending
  // -------------------------------------------------------------------------

  it("keeps the operator signed out and the code on screen while GitHub answers pending", () => {
    // Given
    const backend = aDeviceLoginBackend({ grants: [FIRST_GRANT], polls: [aPendingPoll()] });
    givenTheDeviceSignInPanel(backend);
    deviceLoginPage.startSignIn();
    deviceLoginPage.expectUserCode("WDJB-MJHT");

    // When — two polls, both still pending
    afterSeconds(FIVE_SECOND_INTERVAL);
    expectPollCount(backend, 1);
    afterSeconds(FIVE_SECOND_INTERVAL);
    expectPollCount(backend, 2);

    // Then
    deviceLoginPage.expectSignedOut();
    deviceLoginPage.expectUserCode("WDJB-MJHT");
  });

  it("presents the device code it was given on every poll", () => {
    // Given
    const backend = aDeviceLoginBackend({ grants: [FIRST_GRANT], polls: [aPendingPoll()] });
    givenTheDeviceSignInPanel(backend);
    deviceLoginPage.startSignIn();
    deviceLoginPage.expectUserCode("WDJB-MJHT");

    // When
    afterSeconds(FIVE_SECOND_INTERVAL);
    expectPollCount(backend, 1);
    afterSeconds(FIVE_SECOND_INTERVAL);

    // Then
    expectEveryPollPresented(backend, "3584d83530557fdd1f46af8289938c8ef79f9dc5", 2);
  });

  it("waits the full interval the daemon gave before each poll", () => {
    // Given
    const backend = aDeviceLoginBackend({ grants: [FIRST_GRANT], polls: [aPendingPoll()] });
    givenTheDeviceSignInPanel(backend);
    deviceLoginPage.startSignIn();
    deviceLoginPage.expectUserCode("WDJB-MJHT");

    // When / Then — nothing before the first interval has elapsed, one poll exactly at it
    justBeforeSeconds(FIVE_SECOND_INTERVAL);
    expectPollCount(backend, 0);
    oneMillisecondLater();
    expectPollCount(backend, 1);

    // And — the pending answer restarts the same wait before the second poll
    justBeforeSeconds(FIVE_SECOND_INTERVAL);
    expectPollCount(backend, 1);
    oneMillisecondLater();
    expectPollCount(backend, 2);
  });

  // -------------------------------------------------------------------------
  // Slow down
  // -------------------------------------------------------------------------

  it("widens the poll interval to the one a slow-down answer returns", () => {
    // Given — the first poll is told to slow down to ten seconds
    const backend = aDeviceLoginBackend({
      grants: [FIRST_GRANT],
      polls: [aSlowDownPoll(TEN_SECOND_INTERVAL), aPendingPoll()],
    });
    givenTheDeviceSignInPanel(backend);
    deviceLoginPage.startSignIn();
    deviceLoginPage.expectUserCode("WDJB-MJHT");
    afterSeconds(FIVE_SECOND_INTERVAL);
    expectPollCount(backend, 1);

    // When — the old five-second interval passes, and then the rest of the new one
    afterSeconds(FIVE_SECOND_INTERVAL);
    expectPollCount(backend, 1);
    justBeforeSeconds(TEN_SECOND_INTERVAL - FIVE_SECOND_INTERVAL);
    expectPollCount(backend, 1);
    oneMillisecondLater();

    // Then — the second poll arrives only once the widened interval has elapsed
    expectPollCount(backend, 2);
  });

  it("keeps the widened interval for every poll after a slow-down", () => {
    // Given — slowed to ten seconds on the first poll, pending thereafter
    const backend = aDeviceLoginBackend({
      grants: [FIRST_GRANT],
      polls: [aSlowDownPoll(TEN_SECOND_INTERVAL), aPendingPoll()],
    });
    givenTheDeviceSignInPanel(backend);
    deviceLoginPage.startSignIn();
    deviceLoginPage.expectUserCode("WDJB-MJHT");
    afterSeconds(FIVE_SECOND_INTERVAL);
    expectPollCount(backend, 1);
    afterSeconds(TEN_SECOND_INTERVAL);
    expectPollCount(backend, 2);

    // When — a pending answer, then five seconds
    afterSeconds(FIVE_SECOND_INTERVAL);

    // Then — the pending answer did not snap the interval back to five seconds
    expectPollCount(backend, 2);
    afterSeconds(FIVE_SECOND_INTERVAL);
    expectPollCount(backend, 3);
  });

  // -------------------------------------------------------------------------
  // Complete
  // -------------------------------------------------------------------------

  it("signs the operator in when GitHub reports the code approved", () => {
    // Given
    const backend = aDeviceLoginBackend({ grants: [FIRST_GRANT], polls: [aCompletedPoll()] });
    givenTheDeviceSignInPanel(backend);
    deviceLoginPage.startSignIn();
    deviceLoginPage.expectUserCode("WDJB-MJHT");

    // When
    afterSeconds(FIVE_SECOND_INTERVAL);

    // Then
    deviceLoginPage.expectSignedInAs("octocat");
  });

  it("stores the session tokens exactly where the redirect flow stores them", () => {
    // Given
    const backend = aDeviceLoginBackend({ grants: [FIRST_GRANT], polls: [aCompletedPoll()] });
    givenTheDeviceSignInPanel(backend);
    deviceLoginPage.startSignIn();
    deviceLoginPage.expectUserCode("WDJB-MJHT");

    // When
    afterSeconds(FIVE_SECOND_INTERVAL);
    deviceLoginPage.expectSignedInAs("octocat");

    // Then
    expectStoredSession(DEVICE_LOGIN_ACCESS_TOKEN, DEVICE_LOGIN_REFRESH_TOKEN);
  });

  it("stops polling once the operator is signed in", () => {
    // Given
    const backend = aDeviceLoginBackend({ grants: [FIRST_GRANT], polls: [aCompletedPoll()] });
    givenTheDeviceSignInPanel(backend);
    deviceLoginPage.startSignIn();
    deviceLoginPage.expectUserCode("WDJB-MJHT");
    afterSeconds(FIVE_SECOND_INTERVAL);
    deviceLoginPage.expectSignedInAs("octocat");

    // When — several more intervals pass
    afterSeconds(FIVE_SECOND_INTERVAL * 4);

    // Then
    expectPollCount(backend, 1);
  });

  // -------------------------------------------------------------------------
  // Denied
  // -------------------------------------------------------------------------

  it("ends the attempt with a denial message when the operator refuses at GitHub", () => {
    // Given
    const backend = aDeviceLoginBackend({ grants: [FIRST_GRANT], polls: [aDeniedPoll()] });
    givenTheDeviceSignInPanel(backend);
    deviceLoginPage.startSignIn();
    deviceLoginPage.expectUserCode("WDJB-MJHT");

    // When
    afterSeconds(FIVE_SECOND_INTERVAL);

    // Then — the refusal is named as such, and the dead code is no longer offered
    deviceLoginPage.expectDeniedMessage();
    deviceLoginPage.expectNoExpiredMessage();
    deviceLoginPage.expectNoUserCode();
    deviceLoginPage.expectSignedOut();
  });

  it("stops polling after the operator refuses", () => {
    // Given
    const backend = aDeviceLoginBackend({ grants: [FIRST_GRANT], polls: [aDeniedPoll()] });
    givenTheDeviceSignInPanel(backend);
    deviceLoginPage.startSignIn();
    deviceLoginPage.expectUserCode("WDJB-MJHT");
    afterSeconds(FIVE_SECOND_INTERVAL);
    deviceLoginPage.expectDeniedMessage();

    // When
    afterSeconds(FIVE_SECOND_INTERVAL * 4);

    // Then
    expectPollCount(backend, 1);
  });

  it("offers to start again after a denial, with a fresh code", () => {
    // Given
    const backend = aDeviceLoginBackend({
      grants: [FIRST_GRANT, SECOND_GRANT],
      polls: [aDeniedPoll(), aPendingPoll()],
    });
    givenTheDeviceSignInPanel(backend);
    deviceLoginPage.startSignIn();
    deviceLoginPage.expectUserCode("WDJB-MJHT");
    afterSeconds(FIVE_SECOND_INTERVAL);
    deviceLoginPage.expectDeniedMessage();
    deviceLoginPage.expectOfferedToStartAgain();

    // When
    deviceLoginPage.startSignIn();

    // Then
    expectStartCount(backend, 2);
    deviceLoginPage.expectUserCode("QKRT-4XZN");
    deviceLoginPage.expectNoDeniedMessage();
  });

  // -------------------------------------------------------------------------
  // Expired
  // -------------------------------------------------------------------------

  it("ends the attempt with an expiry message distinct from the denial message", () => {
    // Given
    const backend = aDeviceLoginBackend({ grants: [FIRST_GRANT], polls: [anExpiredPoll()] });
    givenTheDeviceSignInPanel(backend);
    deviceLoginPage.startSignIn();
    deviceLoginPage.expectUserCode("WDJB-MJHT");

    // When
    afterSeconds(FIVE_SECOND_INTERVAL);

    // Then
    deviceLoginPage.expectExpiredMessage();
    deviceLoginPage.expectNoDeniedMessage();
    deviceLoginPage.expectNoUserCode();
    deviceLoginPage.expectSignedOut();
  });

  it("stops polling after the codes expire", () => {
    // Given
    const backend = aDeviceLoginBackend({ grants: [FIRST_GRANT], polls: [anExpiredPoll()] });
    givenTheDeviceSignInPanel(backend);
    deviceLoginPage.startSignIn();
    deviceLoginPage.expectUserCode("WDJB-MJHT");
    afterSeconds(FIVE_SECOND_INTERVAL);
    deviceLoginPage.expectExpiredMessage();

    // When
    afterSeconds(FIVE_SECOND_INTERVAL * 4);

    // Then
    expectPollCount(backend, 1);
  });

  it("offers a new code after the codes expire", () => {
    // Given
    const backend = aDeviceLoginBackend({
      grants: [FIRST_GRANT, SECOND_GRANT],
      polls: [anExpiredPoll(), aPendingPoll()],
    });
    givenTheDeviceSignInPanel(backend);
    deviceLoginPage.startSignIn();
    deviceLoginPage.expectUserCode("WDJB-MJHT");
    afterSeconds(FIVE_SECOND_INTERVAL);
    deviceLoginPage.expectExpiredMessage();
    deviceLoginPage.expectOfferedToStartAgain();

    // When
    deviceLoginPage.startSignIn();

    // Then
    expectStartCount(backend, 2);
    deviceLoginPage.expectUserCode("QKRT-4XZN");
    deviceLoginPage.expectNoExpiredMessage();
  });

  it("polls the new code, not the expired one, after starting again", () => {
    // Given
    const backend = aDeviceLoginBackend({
      grants: [FIRST_GRANT, SECOND_GRANT],
      polls: [anExpiredPoll(), aPendingPoll()],
    });
    givenTheDeviceSignInPanel(backend);
    deviceLoginPage.startSignIn();
    deviceLoginPage.expectUserCode("WDJB-MJHT");
    afterSeconds(FIVE_SECOND_INTERVAL);
    deviceLoginPage.expectExpiredMessage();
    deviceLoginPage.startSignIn();
    deviceLoginPage.expectUserCode("QKRT-4XZN");

    // When
    afterSeconds(FIVE_SECOND_INTERVAL);

    // Then
    expectPollCount(backend, 2);
    cy.wrap(null).should(() => {
      expect(pollsTo(backend)[1].deviceCode).to.equal("7c3f1e0a9b2d4e6f8a1c3e5f7a9b1d3f5e7a9c1e");
    });
  });
});

// ---------------------------------------------------------------------------
// Choosing between the redirect flow and the device flow
// ---------------------------------------------------------------------------

describe("Choosing the GitHub sign-in flow", () => {
  it("reads the device flow from the auth_flow the daemon's /api/config declares", () => {
    // Given
    givenADaemonDeclaringAuthFlow("device");

    // When / Then
    whenThePageReadsItsClientConfig().its("authFlow").should("equal", "device");
  });

  it("reads the redirect flow from the auth_flow the daemon's /api/config declares", () => {
    // Given
    givenADaemonDeclaringAuthFlow("redirect");

    // When / Then
    whenThePageReadsItsClientConfig().its("authFlow").should("equal", "redirect");
  });

  it("offers the device flow, and no redirect button, on a daemon that serves the device flow", () => {
    // Given / When
    givenTheSignInScreenFor("device");

    // Then
    deviceLoginPage.expectDeviceFlowOffered();
    deviceLoginPage.expectNoRedirectButton();
  });

  it("offers the redirect button, and no device flow, on a daemon that serves the redirect flow", () => {
    // Given / When
    givenTheSignInScreenFor("redirect");

    // Then
    deviceLoginPage.expectRedirectButtonOffered();
    deviceLoginPage.expectNoDeviceFlow();
  });

  it("offers the redirect button on a daemon that predates the device flow and declares no flow", () => {
    // Given / When
    givenTheSignInScreenFor(undefined);

    // Then
    deviceLoginPage.expectRedirectButtonOffered();
    deviceLoginPage.expectNoDeviceFlow();
  });
});
