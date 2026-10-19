/**
 * The daemon settings screen shows what the LiveKit integration is doing right now — PRD
 * `docs/ft/web/1-WIP/PRD-2026-10-16-livekit-satellite-status-ui.md`.
 */

import { Code } from "@connectrpc/connect";
import { IntegrationState } from "../../src/gen/daemon_config_pb";
import {
  aDaemonFailingIntegrationStatus,
  aDaemonReportingIntegrations,
  aDaemonSettingsScreen,
  aDaemonWithoutIntegrationStatus,
  aLiveKitIntegration,
} from "../support/drivers/daemonSettingsDriver";

describe("Daemon settings — LiveKit status", () => {
  it("shows LiveKit as unreachable with the server's error", () => {
    aDaemonSettingsScreen(
      aDaemonReportingIntegrations(
        aLiveKitIntegration({
          state: IntegrationState.UNREACHABLE,
          secondsAgo: 12,
          error: "signal connection timed out",
        }),
      ),
    )
      .mount()
      .expectLiveKitStatus("Unreachable · 12s: signal connection timed out");
  });

  it("shows which LiveKit fields are missing", () => {
    aDaemonSettingsScreen(
      aDaemonReportingIntegrations(
        aLiveKitIntegration({
          state: IntegrationState.MISCONFIGURED,
          secondsAgo: 30,
          missingFields: ["api_secret"],
        }),
      ),
    )
      .mount()
      .expectLiveKitStatus("Missing: api_secret");
  });

  it("shows LiveKit as connected", () => {
    aDaemonSettingsScreen(
      aDaemonReportingIntegrations(
        aLiveKitIntegration({ state: IntegrationState.CONNECTED, secondsAgo: 180 }),
      ),
    )
      .mount()
      .expectLiveKitStatus("Connected · 3m");
  });

  it("shows LiveKit as off when it is switched off", () => {
    aDaemonSettingsScreen(
      aDaemonReportingIntegrations(
        aLiveKitIntegration({ state: IntegrationState.DISABLED, secondsAgo: 60 }),
      ),
    )
      .mount()
      .expectLiveKitStatus("Off");
  });

  it("refreshes the status after a save", () => {
    aDaemonSettingsScreen(
      aDaemonReportingIntegrations(
        aLiveKitIntegration({ state: IntegrationState.CONNECTING, secondsAgo: 1 }),
        aLiveKitIntegration({
          state: IntegrationState.UNREACHABLE,
          secondsAgo: 0,
          error: "connection refused",
        }),
      ),
    )
      .mount()
      .expectLiveKitStatus("Connecting…")
      .typeLiveKitUrl("ws://10.0.0.5:7880")
      .save()
      .expectLiveKitStatus("Unreachable · 0s: connection refused");
  });

  it("shows no status for a daemon without the status call", () => {
    aDaemonSettingsScreen(aDaemonWithoutIntegrationStatus())
      .mount()
      .expectLiveKitUrl("ws://127.0.0.1:7880")
      .expectNoLiveKitStatus();
  });

  it("says the status is unavailable when the call fails, without blaming the form", () => {
    aDaemonSettingsScreen(aDaemonFailingIntegrationStatus(Code.Unavailable, "daemon is restarting"))
      .mount()
      .expectLiveKitStatus("Status unavailable: daemon is restarting")
      .expectNoError();
  });
});
