import { describe, expect, test } from "bun:test";
import { join } from "node:path";
import { forbiddenImports } from "../../../scripts/web-package-boundary";

const PACKAGE_DIR = join(import.meta.dir, "..");

describe("tddy-session-web package boundary", () => {
  test("imports nothing from tddy-web or livekit-client", () => {
    // Given the rule every component package follows
    const rules = ["tddy-web", /\/tddy-web\//, "livekit-client", "tddy-livekit-web", "tddy-livekit-rpc-web"];

    // When
    const found = [...forbiddenImports(PACKAGE_DIR, rules), ...forbiddenImports(PACKAGE_DIR, rules, "cypress")];

    // Then
    expect(found).toEqual([]);
  });

  test("exports its public surface", () => {
    // Given
    const exported = Object.keys(require("../src/index.ts"));

    // Then
    expect(exported).toEqual(
      expect.arrayContaining([
        "sessionIndicatorFor",
        "parseSessionParticipantMetadata",
        "requestSessionsRefresh",
        "TrafficMeterRegistry",
        "useSessionNotifications",
      ]),
    );
  });
});
