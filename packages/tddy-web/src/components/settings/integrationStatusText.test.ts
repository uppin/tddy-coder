/**
 * The status line's wording, one case per integration state — PRD
 * `docs/ft/web/1-WIP/PRD-2026-10-16-livekit-satellite-status-ui.md`.
 */

import { describe, it, expect } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { IntegrationState, IntegrationStatusSchema } from "../../gen/daemon_config_pb";
import { integrationStatusText } from "./integrationStatusText";

const NOW = new Date("2026-10-16T20:00:00Z");

function aLiveKitStatus(fields: {
  state: IntegrationState;
  secondsAgo: number;
  error?: string;
  missingFields?: string[];
}) {
  return create(IntegrationStatusSchema, {
    id: "livekit",
    label: "LiveKit",
    state: fields.state,
    error: fields.error ?? "",
    missingFields: fields.missingFields ?? [],
    sinceUnixMs: BigInt(NOW.getTime() - fields.secondsAgo * 1000),
  });
}

describe("integrationStatusText", () => {
  it("says Off for a disabled integration", () => {
    expect(
      integrationStatusText(aLiveKitStatus({ state: IntegrationState.DISABLED, secondsAgo: 5 }), NOW),
    ).toBe("Off");
  });

  it("names the missing fields of a misconfigured integration", () => {
    expect(
      integrationStatusText(
        aLiveKitStatus({
          state: IntegrationState.MISCONFIGURED,
          secondsAgo: 5,
          missingFields: ["url", "api_secret"],
        }),
        NOW,
      ),
    ).toBe("Missing: url, api_secret");
  });

  it("says Connecting… while the first attempt is in flight", () => {
    expect(
      integrationStatusText(aLiveKitStatus({ state: IntegrationState.CONNECTING, secondsAgo: 2 }), NOW),
    ).toBe("Connecting…");
  });

  it("says Connected with how long, in minutes once past a minute", () => {
    expect(
      integrationStatusText(aLiveKitStatus({ state: IntegrationState.CONNECTED, secondsAgo: 180 }), NOW),
    ).toBe("Connected · 3m");
  });

  it("says Unreachable with how long and the server's error", () => {
    expect(
      integrationStatusText(
        aLiveKitStatus({
          state: IntegrationState.UNREACHABLE,
          secondsAgo: 12,
          error: "signal connection timed out",
        }),
        NOW,
      ),
    ).toBe("Unreachable · 12s: signal connection timed out");
  });

  it("measures long-standing states in hours", () => {
    expect(
      integrationStatusText(aLiveKitStatus({ state: IntegrationState.CONNECTED, secondsAgo: 7200 }), NOW),
    ).toBe("Connected · 2h");
  });
});
