import type { mount } from "cypress/react";
import type { TerminalSessionResult } from "./cypress/support/commands";

declare global {
  namespace Cypress {
    interface Chainable {
      mount: typeof mount;

      /**
       * Start a terminal server session and yield its connection details.
       * Skips the current suite when LIVEKIT_TESTKIT_WS_URL is not set.
       *
       * kind "terminal" → startTerminalServer task
       * kind "echo"     → startEchoTerminal task
       */
      startTerminalSession(opts: {
        kind: "terminal" | "echo";
        prompt?: string;
        roomName?: string;
      }): Chainable<TerminalSessionResult>;

      /**
       * Start the tddy-coder app for connect-flow e2e tests.
       * Returns the server base URL.
       */
      startTddyCoderApp(opts: { flow: "connect" }): Chainable<{ baseUrl: string }>;

      /**
       * Navigate to a GhosttyTerminal Storybook iframe story with LiveKit
       * connection params encoded in the query string.
       */
      visitGhosttyStory(opts: {
        storyId: string;
        url: string;
        token: string;
        roomName: string;
        extra?: Record<string, string>;
      }): Chainable<void>;

      /**
       * Wait for the full LiveKit terminal connection:
       * status-dot connected + livekit-status hidden + ghostty-terminal exists
       * + first-output-received exists.
       */
      connectAndWaitForTerminal(): Chainable<void>;

      /**
       * Retrying assertion: wait for the terminal buffer text to include
       * the given substring. Avoids fixed cy.wait() sleeps.
       */
      waitForBufferText(
        substr: string,
        opts?: { timeout?: number },
      ): Chainable<void>;

      /**
       * Read a server log file and emit its contents to the Cypress task log.
       */
      dumpServerLog(logPath: string): Chainable<void>;

      /**
       * Clear localStorage and sessionStorage.
       */
      clearAppStorage(): Chainable<void>;
    }
  }
}
