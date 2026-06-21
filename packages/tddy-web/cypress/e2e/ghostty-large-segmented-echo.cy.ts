/**
 * E2E: Ghostty + LiveKit + echo_terminal — large segmented payload assertion.
 *
 * Mirrors the contiguous-prefix check in tddy-e2e/tests/grpc_terminal_rpc.rs.
 *
 * echo_terminal echoes a full line after Enter (not char-by-char), so the
 * payload is typed and submitted with Enter. Assertions use the hidden
 * `terminal-buffer-text` element (Ghostty buffer text).
 *
 * Requires: LIVEKIT_TESTKIT_WS_URL, cargo build -p tddy-livekit --example echo_terminal,
 * Storybook static bundle (bun run build-storybook).
 * Skipped when LIVEKIT_TESTKIT_WS_URL is not set.
 */
import type { TerminalSessionResult } from "../support/commands";
import {
  buildLargeEchoSegmentedPayload,
  compactNoWs,
  longestContiguousPrefixLen,
} from "../support/util/terminalText";

const LARGE_ECHO_CHAR_CAP = 1000;
const LARGE_ECHO_SEGMENTS = 10;
const LARGE_ECHO_E2E_ROOM = "large-echo-e2e";

const STORY_ID = "components-ghosttyterminal--live-kit-echo-large-segmented";

describe("Ghostty large segmented echo (LiveKit + echo_terminal)", () => {
  let session: TerminalSessionResult = {} as TerminalSessionResult;

  before(function () {
    cy.startTerminalSession({ kind: "echo", roomName: LARGE_ECHO_E2E_ROOM }).then((result) => {
      session = result;
    });
  });

  after(() => {
    cy.task("stopEchoTerminal");
  });

  it("shows the full segmented payload in the terminal buffer after line submit (matches Rust oracle)", () => {
    // Given
    cy.viewport(1400, 900);
    const { full: expected, segments } = buildLargeEchoSegmentedPayload(
      LARGE_ECHO_CHAR_CAP,
      LARGE_ECHO_SEGMENTS,
    );
    const expectedNoWs = compactNoWs(expected);

    cy.visitGhosttyStory({ storyId: STORY_ID, url: session.url, token: session.clientToken, roomName: session.roomName });

    // When — wait for connection and first output
    cy.connectAndWaitForTerminal();
    cy.get("[data-testid='ghostty-terminal']").click();
    cy.get("[data-testid='ghostty-terminal']").type(expected, { delay: 0 });
    cy.get("[data-testid='ghostty-terminal']").type("{enter}");

    // Then — full segmented payload echoed contiguously
    cy.get("[data-testid='terminal-buffer-text']", { timeout: 120000 }).should(($el) => {
      const compact = compactNoWs($el.text());
      const lo = longestContiguousPrefixLen(compact, expectedNoWs);
      const segFlags = segments.map((seg) => compact.includes(compactNoWs(seg)));
      const markerFlags = segments.map((_, i) => compact.includes(`#SEG-${i}:`));
      expect(
        lo,
        `contiguous echo prefix: ${lo} of ${expectedNoWs.length}; per-segment full: ${JSON.stringify(segFlags)}; markers: ${JSON.stringify(markerFlags)}`,
      ).to.eq(expectedNoWs.length);
    });
  });
});
