/**
 * Acceptance tests for the daemon settings screen.
 *
 * Uses the in-memory ConnectRPC backend (not `cy.intercept`) so each test asserts on the typed
 * `DaemonConfigService` request the screen actually sent.
 *
 * See `docs/dev/1-WIP/2026-09-04-tauri-desktop-single-process.md` for the design this validates.
 */

import { Code } from "@connectrpc/connect";
import { DaemonConfigService } from "../../src/gen/daemon_config_pb";
import {
  aDaemonConfigBackend,
  aDaemonRefusingUpdates,
  aDaemonSettingsScreen,
  aDaemonThatCannotApply,
} from "../support/drivers/daemonSettingsDriver";

describe("Daemon settings", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("shows the effective LiveKit configuration with the API secret masked", () => {
    // Given a daemon holding a LiveKit secret
    const screen = aDaemonSettingsScreen(aDaemonConfigBackend());

    // When the settings screen loads
    screen.mount();

    // Then the URL and key are shown, and the secret is reported as held without being shown
    screen
      .expectLiveKitUrl("ws://127.0.0.1:7880")
      .expectLiveKitApiKey("devkey")
      .expectSecretHeldButNotShown("the-secret");
  });

  it("saves an edited LiveKit URL through UpdateConfig", () => {
    // Given the settings screen loaded
    const backend = aDaemonConfigBackend();
    const screen = aDaemonSettingsScreen(backend).mount();

    // When the LiveKit URL is edited and saved
    screen.typeLiveKitUrl("ws://10.0.0.5:7880").save();

    // Then the daemon was asked to persist exactly that
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(DaemonConfigService.method.updateConfig);
      expect(calls).to.have.length(1);
      expect(calls[0].settings?.livekit?.url).to.equal("ws://10.0.0.5:7880");
    });
  });

  it("lists the fields that need a restart after saving", () => {
    // Given a daemon that cannot apply a web-port change while running
    const screen = aDaemonSettingsScreen(
      aDaemonThatCannotApply(["listen.web_port"]),
    ).mount();

    // When settings are saved
    screen.save();

    // Then the screen names the field that is waiting on a restart
    screen.expectRestartRequired("listen.web_port");
  });

  it("keeps the entered values and shows the daemon's validation error when the update is rejected", () => {
    // Given a daemon that refuses a LiveKit URL which is not a websocket URL
    const screen = aDaemonSettingsScreen(
      aDaemonRefusingUpdates(
        Code.InvalidArgument,
        "livekit.url must be a ws:// or wss:// URL",
      ),
    ).mount();

    // When such a URL is entered and saved
    screen.typeLiveKitUrl("http://10.0.0.5:7880").save();

    // Then the refusal is shown and the entered value is still there to correct
    screen
      .expectError("must be a ws:// or wss:// URL")
      .expectLiveKitUrl("http://10.0.0.5:7880");
  });
});
