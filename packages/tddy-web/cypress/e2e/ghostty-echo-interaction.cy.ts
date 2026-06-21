/**
 * E2E: Ghostty terminal interaction with the echo_terminal app.
 *
 * Reproduces: "after 1st key press everything breaks and random garbage chars".
 *
 * Flow:
 * 1. Start echo_terminal (greeting + echo each line)
 * 2. Visit LiveKitEcho story
 * 3. OCR: assert greeting visible on terminal
 * 4. Type a line, press Enter
 * 5. OCR: assert echoed line visible (not garbage)
 *
 * Requires: LIVEKIT_TESTKIT_WS_URL, echo_terminal built.
 * Skipped when LIVEKIT_TESTKIT_WS_URL is not set.
 */
import type { TerminalSessionResult } from "../support/commands";

const SCREENSHOT_SPEC_DIR = "cypress/screenshots/ghostty-echo-interaction.cy.ts";
const STORY_ID = "components-ghosttyterminal--live-kit-echo";
const TYPED_LINE = "cypress-test-line";

describe("Ghostty Echo Interaction", () => {
  let session: TerminalSessionResult = {} as TerminalSessionResult;

  before(function () {
    cy.startTerminalSession({ kind: "echo" }).then((result) => {
      session = result;
    });
  });

  after(() => {
    if (session.echoTerminalLogPath) {
      cy.dumpServerLog(session.echoTerminalLogPath);
    }
    cy.task("stopEchoTerminal");
  });

  it("shows greeting then echoes the typed line without garbage characters", () => {
    // Given — visit the echo story with debug logging enabled
    const debugLogs: string[] = [];
    cy.visitGhosttyStory({ storyId: STORY_ID, url: session.url, token: session.clientToken, roomName: session.roomName, extra: { debugLogging: "1" } });
    cy.window().then((win) => {
      const orig = win.console.log;
      win.console.log = (...args: unknown[]) => {
        orig.apply(win.console, args);
        debugLogs.push(args.map((a) => (typeof a === "object" ? JSON.stringify(a) : String(a))).join(" "));
      };
    });

    // When — wait for full connection and first output
    cy.connectAndWaitForTerminal();

    // Emit captured debug logs at this synchronisation point
    cy.then(() => {
      if (debugLogs.length > 0) {
        cy.task("log", ["\n--- Debug logs (after connect) ---", ...debugLogs, "--- End debug logs ---\n"].join("\n"));
      }
    });

    // Then — greeting is visible via OCR (brief settle wait before screenshot — OCR needs stable canvas)
    // eslint-disable-next-line cypress/no-unnecessary-waiting
    cy.wait(500); // justified: OCR reads a screenshot; terminal must be visually settled first
    cy.get("[data-testid='ghostty-terminal']").screenshot("greeting");
    cy.task("ocrScreenshot", `${SCREENSHOT_SPEC_DIR}/greeting.png`).then((ocrText: unknown) => {
      expect(String(ocrText ?? "")).to.include("Hello", "greeting should be visible after load");
    });

    // When — type a test line and submit
    cy.get("[data-testid='ghostty-terminal']").click();
    cy.get("[data-testid='ghostty-terminal']").type(`${TYPED_LINE}{enter}`);

    // Brief settle before OCR screenshot (echo round-trip takes time across LiveKit)
    // eslint-disable-next-line cypress/no-unnecessary-waiting
    cy.wait(1000); // justified: echo_terminal echoes the full line after Enter; round-trip latency
    cy.get("[data-testid='ghostty-terminal']").screenshot("echoed");

    // Then — typed line echoed back without garbage
    cy.task("ocrScreenshot", `${SCREENSHOT_SPEC_DIR}/echoed.png`).then((ocrText: unknown) => {
      expect(String(ocrText ?? "")).to.include(TYPED_LINE, "typed line should be echoed back, not garbage");
    });

    // Emit full debug log on completion
    cy.then(() => {
      if (debugLogs.length > 0) {
        cy.task("log", ["\n--- Debug logs (after full flow) ---", ...debugLogs, "--- End debug logs ---\n"].join("\n"));
      }
    });
  });
});
