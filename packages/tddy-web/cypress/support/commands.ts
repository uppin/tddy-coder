/**
 * E2E custom Cypress commands.
 *
 * Each command encapsulates a recurring setup or interaction pattern that
 * appears in multiple e2e specs.
 */

/** Maximum time (ms) to wait for the LiveKit connection status-dot to reach "connected". */
const TERMINAL_CONNECT_TIMEOUT_MS = 25000;

/** Maximum time (ms) to wait for first terminal output after connection. */
const FIRST_OUTPUT_TIMEOUT_MS = 15000;

// ---------------------------------------------------------------------------
// Terminal server lifecycle
// ---------------------------------------------------------------------------

export interface TerminalSessionResult {
  url: string;
  clientToken: string;
  roomName: string;
  serverLogPath?: string;
  echoTerminalLogPath?: string;
}

/**
 * Start a terminal server (LiveKit-based or echo variant) and yield its
 * connection details. Skips the current suite when LIVEKIT_TESTKIT_WS_URL is
 * not set.
 *
 * kind "terminal"  → cy.task("startTerminalServer", { prompt? })
 * kind "echo"      → cy.task("startEchoTerminal", { roomName? })
 */
Cypress.Commands.add(
  "startTerminalSession",
  function (opts: {
    kind: "terminal" | "echo";
    prompt?: string;
    roomName?: string;
  }): Cypress.Chainable<TerminalSessionResult> {
    if (!Cypress.env("LIVEKIT_TESTKIT_WS_URL")) {
      this.skip();
      // Return a never-resolving chain so callers can still call .then() safely.
      return cy.wrap(undefined as unknown as TerminalSessionResult);
    }

    const taskName =
      opts.kind === "terminal" ? "startTerminalServer" : "startEchoTerminal";
    const taskArgs: Record<string, string | undefined> = {};
    if (opts.kind === "terminal" && opts.prompt !== undefined) {
      taskArgs["prompt"] = opts.prompt;
    }
    if (opts.kind === "echo" && opts.roomName !== undefined) {
      taskArgs["roomName"] = opts.roomName;
    }

    return cy
      .task(taskName, taskArgs)
      .then((result) => result as TerminalSessionResult);
  },
);

/**
 * Start the tddy-coder process for app-level connect tests.
 *
 * flow "connect" → cy.task("startTddyCoderForConnectFlow")
 *
 * Returns the base URL of the started server.
 */
Cypress.Commands.add(
  "startTddyCoderApp",
  function (opts: { flow: "connect" }): Cypress.Chainable<{ baseUrl: string }> {
    if (!Cypress.env("LIVEKIT_TESTKIT_WS_URL")) {
      this.skip();
      return cy.wrap({ baseUrl: "" });
    }
    return cy
      .task("startTddyCoderForConnectFlow")
      .then((result) => result as { baseUrl: string });
  },
);

// ---------------------------------------------------------------------------
// Story navigation
// ---------------------------------------------------------------------------

/**
 * Navigate to a GhosttyTerminal Storybook iframe story with the given
 * LiveKit connection params encoded in the URL.
 *
 * Example:
 *   cy.visitGhosttyStory({ storyId: "components-ghosttyterminal--live-kit-connected", url, token, roomName })
 */
Cypress.Commands.add(
  "visitGhosttyStory",
  function (opts: {
    storyId: string;
    url: string;
    token: string;
    roomName: string;
    extra?: Record<string, string>;
  }): void {
    const params = new URLSearchParams({
      id: opts.storyId,
      url: opts.url,
      token: opts.token,
      roomName: opts.roomName,
      ...opts.extra,
    });
    cy.visit(`/iframe.html?${params.toString()}`);
  },
);

// ---------------------------------------------------------------------------
// Terminal connection waits
// ---------------------------------------------------------------------------

/**
 * Wait for the full LiveKit terminal connection handshake:
 *   1. connection-status-dot reaches data-connection-status="connected"
 *   2. livekit-status is hidden
 *   3. ghostty-terminal exists
 *   4. first-output-received exists
 */
Cypress.Commands.add("connectAndWaitForTerminal", function (): void {
  cy.get("[data-testid='connection-status-dot']", {
    timeout: TERMINAL_CONNECT_TIMEOUT_MS,
  })
    .should("be.visible")
    .and("have.attr", "data-connection-status", "connected");

  cy.get("[data-testid='livekit-status']").should("not.be.visible");

  cy.get("[data-testid='ghostty-terminal']", { timeout: 5000 }).should("exist");

  cy.get("[data-testid='first-output-received']", {
    timeout: FIRST_OUTPUT_TIMEOUT_MS,
  }).should("exist");
});

/**
 * Wait (with retrying assertion) for the terminal buffer text to include the
 * given substring. Prefers this over fixed cy.wait() sleeps.
 */
Cypress.Commands.add(
  "waitForBufferText",
  function (
    substr: string,
    opts?: { timeout?: number },
  ): void {
    cy.get("[data-testid='terminal-buffer-text']", {
      timeout: opts?.timeout ?? 20000,
    }).should(($el) => {
      expect($el.text()).to.include(substr);
    });
  },
);

// ---------------------------------------------------------------------------
// Teardown helpers
// ---------------------------------------------------------------------------

/**
 * Read a server log file and emit its contents to the Cypress task log.
 */
Cypress.Commands.add("dumpServerLog", function (logPath: string): void {
  cy.task("readLogFile", logPath).then((content) => {
    cy.task("log", `\n--- server log ---\n${content}\n--- end server log ---\n`);
  });
});

/**
 * Clear localStorage and sessionStorage. Registered as a command so it can
 * be composed and is callable from beforeEach in the global e2e setup.
 */
Cypress.Commands.add("clearAppStorage", function (): void {
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
});
