/**
 * Fluent driver for the daemon settings screen: mounting, editing and asserting, so the tests
 * stay free of selectors and RPC wiring.
 */

import React from "react";
import { createClient, type Code } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { DaemonSettingsScreen } from "../../../src/components/settings/DaemonSettingsScreen";
import { DaemonConfigService } from "../../../src/gen/daemon_config_pb";
import { TEST_IDS, byTestId } from "../testIds";

export const SESSION_TOKEN = "tok";

/** The configuration the daemon under test is running with. */
export function aDaemonConfiguration() {
  return {
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
  };
}

/** Serves `aDaemonConfiguration()`; how it answers an update is each factory's business. */
function aDaemonServing() {
  return anInMemoryRpcBackend().onUnary(
    DaemonConfigService.method.getConfig,
    () => aDaemonConfiguration(),
  );
}

/** A daemon that applies every update to itself immediately. */
export function aDaemonConfigBackend() {
  return aDaemonServing().onUnary(DaemonConfigService.method.updateConfig, () => ({
    restartRequired: [],
  }));
}

/** A daemon that persists an update but cannot apply `fields` while running. */
export function aDaemonThatCannotApply(fields: string[]) {
  return aDaemonServing().onUnary(DaemonConfigService.method.updateConfig, () => ({
    restartRequired: fields,
  }));
}

/** A daemon that refuses updates with `message`. */
export function aDaemonRefusingUpdates(code: Code, message: string) {
  return aDaemonServing().failWith(DaemonConfigService.method.updateConfig, code, message);
}

type Backend = ReturnType<typeof aDaemonConfigBackend>;

export function aDaemonSettingsScreen(backend: Backend) {
  const driver = {
    mount() {
      cy.mount(
        <DaemonSettingsScreen
          client={createClient(DaemonConfigService, backend.transport())}
          sessionToken={SESSION_TOKEN}
        />,
      );
      return driver;
    },
    typeLiveKitUrl(url: string) {
      byTestId(TEST_IDS.daemonSettingsLivekitUrl).clear().type(url);
      return driver;
    },
    save() {
      byTestId(TEST_IDS.daemonSettingsSave).click();
      return driver;
    },
    expectLiveKitUrl(url: string) {
      byTestId(TEST_IDS.daemonSettingsLivekitUrl).should("have.value", url);
      return driver;
    },
    expectLiveKitApiKey(apiKey: string) {
      byTestId(TEST_IDS.daemonSettingsLivekitApiKey).should("have.value", apiKey);
      return driver;
    },
    expectSecretHeldButNotShown(secret: string) {
      byTestId(TEST_IDS.daemonSettingsLivekitSecretState)
        .should("be.visible")
        .and("not.contain.text", secret);
      return driver;
    },
    expectRestartRequired(field: string) {
      byTestId(TEST_IDS.daemonSettingsRestartRequired).should("contain.text", field);
      return driver;
    },
    expectError(fragment: string) {
      byTestId(TEST_IDS.daemonSettingsError).should("contain.text", fragment);
      return driver;
    },
  };
  return driver;
}
