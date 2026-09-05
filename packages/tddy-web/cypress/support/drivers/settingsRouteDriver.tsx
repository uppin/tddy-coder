/**
 * Fluent driver for the `#/settings` route: the daemon settings screen as the running app reaches
 * it — through the real router, behind the real sign-in gate, over the page's own daemon client.
 *
 * The screen's own behaviour is driven by `./daemonSettingsDriver`; this one is about the route.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { Room } from "livekit-client";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { App } from "../../../src/index";
import { AuthProvider } from "../../../src/hooks/authProvider";
import { AuthService, GetAuthStatusResponseSchema } from "../../../src/gen/auth_pb";
import { DaemonConfigService, GetConfigResponseSchema } from "../../../src/gen/daemon_config_pb";
import { aGitHubUser } from "../rpc/responses";
import { mountWithRpc } from "../rpc/inMemory";
import { byTestId, TEST_IDS } from "../testIds";

/** The token the signed-in operator's browser holds. */
export const A_SIGNED_IN_SESSION_TOKEN = "signed-in-token";

/** The instance id the daemon serving this page reports in its client configuration. */
const THE_SERVING_DAEMON = { instanceId: "local", label: "local (this daemon)" };

/** The configuration the daemon serving this page is running with. */
export function aServingDaemonsConfiguration() {
  return create(GetConfigResponseSchema, {
    settings: {
      livekit: {
        url: "ws://127.0.0.1:7880",
        publicUrl: "ws://127.0.0.1:7880",
        apiKey: "devkey",
        commonRoom: "tddy-lobby",
        apiSecretSet: true,
      },
      listen: { webPort: 8899, webHost: "127.0.0.1" },
    },
    configPath: "/etc/tddy/daemon.yaml",
  });
}

/** A signed-in operator's daemon, serving its own configuration on the page's own client. */
export function aServingDaemonWithASignedInOperator() {
  return anInMemoryRpcBackend()
    .onUnary(AuthService.method.getAuthStatus, () =>
      create(GetAuthStatusResponseSchema, { authenticated: true, user: aGitHubUser() }),
    )
    .onUnary(DaemonConfigService.method.getConfig, () => aServingDaemonsConfiguration());
}

type Backend = ReturnType<typeof aServingDaemonWithASignedInOperator>;

export function theApp(backend: Backend) {
  const driver = {
    /**
     * Open `path` with the operator already signed in, as a reload of a bookmarked hash does.
     * `/api/config` is the plain HTTP endpoint the daemon serves beside the bundle — not RPC — so
     * it is stubbed at the network, while every RPC goes through `backend`.
     */
    openedAt(path: string) {
      cy.intercept("GET", "**/api/config", {
        statusCode: 200,
        headers: { "Content-Type": "application/json" },
        body: { daemon_mode: true, daemon_instance_id: THE_SERVING_DAEMON.instanceId },
      }).as("apiConfig");
      cy.then(() => {
        window.localStorage.setItem("tddy_session_token", A_SIGNED_IN_SESSION_TOKEN);
        window.location.hash = path;
      });
      mountWithRpc(
        <AuthProvider>
          <App testDaemonRoom={new Room()} testDaemonHosts={[THE_SERVING_DAEMON]} />
        </AuthProvider>,
        backend,
      );
      return driver;
    },
    expectLiveKitUrl(url: string) {
      byTestId(TEST_IDS.daemonSettingsLivekitUrl, { timeout: 10000 }).should("have.value", url);
      return driver;
    },
    expectConfigurationReadWith(sessionToken: string) {
      byTestId(TEST_IDS.daemonSettingsLivekitUrl, { timeout: 10000 }).should("exist");
      cy.then(() =>
        expect(
          backend.callsTo(DaemonConfigService.method.getConfig).map((call) => call.sessionToken),
        ).to.deep.equal([sessionToken]),
      );
      return driver;
    },
  };
  return driver;
}
