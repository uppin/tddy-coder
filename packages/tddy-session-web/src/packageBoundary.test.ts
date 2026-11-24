import { beforeAll, describe, expect, test } from "bun:test";
import { join } from "node:path";
import { forbiddenImports } from "../../../scripts/web-package-boundary";

const PACKAGE_DIR = join(import.meta.dir, "..");

/**
 * Loading the entry point is a cold import of the package's whole module graph, and that — not the
 * assertion — is what costs: measured 0.9–5.8 s on a dev machine at load average 35–90, and 11.3 s
 * at ~370, against bun's 5 s per-test default. It is loaded once, before the tests, with a budget
 * sized to that work.
 */
const ENTRY_POINT_LOAD_BUDGET_MS = 60_000;
let exported: string[] = [];

describe("tddy-session-web package boundary", () => {
  beforeAll(() => {
    exported = Object.keys(require("../src/index.ts"));
  }, ENTRY_POINT_LOAD_BUDGET_MS);

  test("imports nothing from tddy-web or livekit-client", () => {
    // Given the rule every component package follows
    const rules = ["tddy-web", /\/tddy-web\//, "livekit-client", "tddy-livekit-web", "tddy-livekit-rpc-web"];

    // When
    const found = [...forbiddenImports(PACKAGE_DIR, rules), ...forbiddenImports(PACKAGE_DIR, rules, "cypress")];

    // Then
    expect(found).toEqual([]);
  });

  test("exports its public surface", () => {
    // Given the entry point, loaded before the tests

    // Then
    expect(exported).toEqual(
      expect.arrayContaining([
        "sessionIndicatorFor",
        "parseSessionParticipantMetadata",
        "requestSessionsRefresh",
        "TrafficMeterRegistry",
        "useSessionNotifications",
        // session-runtime (#860)
        "SessionRuntime",
        "SessionTerminalTabs",
        "GrpcSessionTerminal",
        "SessionRuntimeRegistry",
        "SessionChildRuntime",
        // session-inspector (#861) — incl. SessionMainPane and its prerequisites
        "SessionMainPane",
        "SessionInspectorDrawer",
        "SessionAgentRosterPane",
        "SessionAgentTree",
        "SessionWithActivitySidebar",
        "WorktreeCodePane",
        "SessionIndexingIndicator",
        "AgentPicker",
        // session-drawer (#862)
        "SessionsDrawerScreen",
        "SessionDrawer",
        "ProjectHomeEntry",
        "StatusBar",
        "SessionTrafficStrip",
        "NewTerminalMenu",
      ]),
    );
  });
});
