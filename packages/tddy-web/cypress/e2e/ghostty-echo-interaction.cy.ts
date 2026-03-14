/**
 * E2E: Ghostty terminal interaction with echo app.
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
 * Requires: LIVEKIT_TESTKIT_WS_URL, echo_terminal built
 */

const SCREENSHOT_SPEC_DIR = "cypress/screenshots/ghostty-echo-interaction.cy.ts";

describe("Ghostty Echo Interaction", () => {
  let serverUrl: string;
  let clientToken: string;
  let roomName: string;

  const TYPED_LINE = "cypress-test-line";

  let echoTerminalLogPath: string | undefined;

  before(function () {
    if (!Cypress.env("LIVEKIT_TESTKIT_WS_URL")) {
      this.skip();
      return;
    }
    return cy.task("startEchoTerminal").then((result) => {
      const r = result as {
        url: string;
        clientToken: string;
        roomName: string;
        echoTerminalLogPath?: string;
      };
      serverUrl = r.url;
      clientToken = r.clientToken;
      roomName = r.roomName;
      echoTerminalLogPath = r.echoTerminalLogPath;
    });
  });

  after(() => {
    if (echoTerminalLogPath) {
      cy.task("readLogFile", echoTerminalLogPath).then((content) => {
        cy.task("log", `\n--- echo_terminal server log (full) ---\n${content}\n--- End server log ---\n`);
      });
    }
    cy.task("stopEchoTerminal");
  });

  it("shows greeting then echoes typed line without garbage", () => {
    const storyUrl = `/iframe.html?id=components-ghosttyterminal--live-kit-echo&url=${encodeURIComponent(serverUrl)}&token=${encodeURIComponent(clientToken)}&roomName=${encodeURIComponent(roomName)}&debugLogging=1`;
    const debugLogs: string[] = [];
    cy.visit(storyUrl, {
      onBeforeLoad(win) {
        const orig = win.console.log;
        win.console.log = (...args: unknown[]) => {
          orig.apply(win.console, args);
          const msg = args.map((a) => (typeof a === "object" ? JSON.stringify(a) : String(a))).join(" ");
          debugLogs.push(msg);
        };
      },
    });

    cy.get("body", { timeout: 10000 }).should("be.visible");

    cy.get(
      "[data-testid='livekit-status'], [data-testid='livekit-placeholder'], [data-testid='livekit-error'], [data-testid='ghostty-terminal']",
      { timeout: 25000 }
    ).should("exist");

    cy.get("[data-testid='livekit-status']", { timeout: 10000 })
      .should("exist")
      .and("have.text", "connected");

    cy.then(() => {
      if (debugLogs.length > 0) {
        const block = ["\n--- Debug logs (after connect) ---", ...debugLogs, "--- End debug logs ---\n"].join("\n");
        cy.task("log", block);
      }
    });

    cy.get("[data-testid='ghostty-terminal']", { timeout: 5000 }).should(
      "exist"
    );
    cy.get("[data-testid='first-output-received']", { timeout: 10000 }).should(
      "exist"
    );
    cy.wait(500);
    cy.get("[data-testid='ghostty-terminal']").screenshot("greeting");

    cy.task("ocrScreenshot", `${SCREENSHOT_SPEC_DIR}/greeting.png`).then(
      (ocrText: unknown) => {
        const text = String(ocrText ?? "");
        expect(text).to.include("Hello", "greeting should be visible after load");
      }
    );

    cy.get("[data-testid='ghostty-terminal']").click();
    cy.get("[data-testid='ghostty-terminal']").type(`${TYPED_LINE}{enter}`);
    cy.wait(1000);
    cy.get("[data-testid='ghostty-terminal']").screenshot("echoed");

    cy.task("ocrScreenshot", `${SCREENSHOT_SPEC_DIR}/echoed.png`).then(
      (ocrText: unknown) => {
        const text = String(ocrText ?? "");
        expect(text).to.include(
          TYPED_LINE,
          "typed line should be echoed back, not garbage"
        );
      }
    );

    cy.then(() => {
      if (debugLogs.length > 0) {
        const block = ["\n--- Debug logs (after full flow) ---", ...debugLogs, "--- End debug logs ---\n"].join("\n");
        cy.task("log", block);
      }
      if (echoTerminalLogPath) {
        cy.task("readLogFile", echoTerminalLogPath).then((content) => {
          cy.task("log", `\n--- echo_terminal server log ---\n${content}\n--- End server log ---\n`);
        });
      }
    });
  });
});
