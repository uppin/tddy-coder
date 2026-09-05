/**
 * Unit tests for the settings form mapping — in particular the two rules that would silently
 * destroy a daemon's credentials if they were wrong: a secret the daemon never returns must not
 * come back as an empty string, and an update must carry every field, not only the edited one.
 *
 * See `docs/dev/1-WIP/2026-09-04-tauri-desktop-single-process.md` for the design this validates.
 */

import { describe, it, expect } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { GetConfigResponseSchema } from "../../gen/daemon_config_pb";
import {
  toFormState,
  toUpdateSettings,
  type DaemonSettingsFormState,
} from "./settingsForm";

const CONFIGURED_URL = "ws://127.0.0.1:7880";

/** The daemon's answer to `GetConfig`: a LiveKit block with a stored, unreturned secret. */
function aConfiguredDaemonResponse() {
  return create(GetConfigResponseSchema, {
    configPath: "/etc/tddy/daemon.yaml",
    settings: {
      livekit: {
        url: CONFIGURED_URL,
        publicUrl: CONFIGURED_URL,
        apiKey: "devkey",
        commonRoom: "tddy-lobby",
        apiSecretSet: true,
      },
      listen: { webPort: 8899, webHost: "127.0.0.1" },
    },
  });
}

/** The form as it stands after loading `aConfiguredDaemonResponse()`. */
function theLoadedForm(
  overrides: Partial<DaemonSettingsFormState> = {},
): DaemonSettingsFormState {
  return {
    livekitUrl: CONFIGURED_URL,
    livekitPublicUrl: CONFIGURED_URL,
    livekitApiKey: "devkey",
    livekitApiSecret: "",
    livekitCommonRoom: "tddy-lobby",
    livekitApiSecretSet: true,
    webPort: "8899",
    webHost: "127.0.0.1",
    ...overrides,
  };
}

describe("daemon settings form", () => {
  it("fills the form from the daemon's effective configuration", () => {
    // Given the daemon's current configuration
    const response = aConfiguredDaemonResponse();

    // When the form is loaded from it
    const form = toFormState(response);

    // Then every editable field is filled from it
    expect(form.livekitUrl).toEqual(CONFIGURED_URL);
    expect(form.livekitApiKey).toEqual("devkey");
    expect(form.livekitCommonRoom).toEqual("tddy-lobby");
    expect(form.webPort).toEqual("8899");
    expect(form.webHost).toEqual("127.0.0.1");
  });

  it("marks the API secret as stored while leaving the secret field blank", () => {
    // Given a daemon holding an API secret it does not return
    const response = aConfiguredDaemonResponse();

    // When the form is loaded from it
    const form = toFormState(response);

    // Then the operator can see one is set, and has nothing to accidentally re-send
    expect(form.livekitApiSecretSet).toEqual(true);
    expect(form.livekitApiSecret).toEqual("");
  });

  it("leaves the form blank for a daemon with no LiveKit block", () => {
    // Given a daemon configured without LiveKit
    const response = create(GetConfigResponseSchema, {
      configPath: "/etc/tddy/daemon.yaml",
      settings: { listen: { webPort: 8899 } },
    });

    // When the form is loaded from it
    const form = toFormState(response);

    // Then LiveKit reads as unconfigured rather than as an empty server address
    expect(form.livekitUrl).toEqual("");
    expect(form.livekitApiSecretSet).toEqual(false);
  });

  it("omits the API secret from an update when the field was left blank", () => {
    // Given a form where only the URL was edited
    const form = theLoadedForm({ livekitUrl: "ws://10.0.0.5:7880" });

    // When the update is built
    const settings = toUpdateSettings(form);

    // Then no secret is sent — an empty string would clear the daemon's credentials
    expect(settings.livekit?.apiSecret).toBeUndefined();
    expect(settings.livekit?.url).toEqual("ws://10.0.0.5:7880");
  });

  it("sends a newly typed API secret in the update", () => {
    // Given a form where the operator typed a new secret
    const form = theLoadedForm({ livekitApiSecret: "rotated" });

    // When the update is built
    const settings = toUpdateSettings(form);

    // Then it carries the new secret
    expect(settings.livekit?.apiSecret).toEqual("rotated");
  });

  it("carries every field an update replaces, not only the edited one", () => {
    // Given a form where only the common room was edited
    const form = theLoadedForm({ livekitCommonRoom: "tddy-other" });

    // When the update is built
    const settings = toUpdateSettings(form);

    // Then the untouched fields go back too, because an update replaces the whole message
    expect(settings.livekit?.url).toEqual(CONFIGURED_URL);
    expect(settings.livekit?.apiKey).toEqual("devkey");
    expect(settings.livekit?.commonRoom).toEqual("tddy-other");
    expect(settings.listen?.webHost).toEqual("127.0.0.1");
  });

  it("sends the web port as a number", () => {
    // Given a form whose port field is text, as every input's value is
    const form = theLoadedForm({ webPort: "9911" });

    // When the update is built
    const settings = toUpdateSettings(form);

    // Then the port crosses the wire as a number
    expect(settings.listen?.webPort).toEqual(9911);
  });
});
