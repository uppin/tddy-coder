/**
 * E2E: the PR-Stack Chat Screen must show the agent's response.
 *
 * Full stack — real tddy-daemon wired to the LiveKit testkit, a real pr-stack session spawned with
 * the deterministic `stub` backend (the same backend `tddy-demo` uses). Reproduces the reported bug:
 * opening a pr-stack session and sending "hi" shows no agent response in the chat.
 */

import { prStackScreenPage } from "../support/pages/prStackScreenPage";

const SESSION_TOKEN_KEY = "tddy_session_token";
// Deterministic first line of StubBackend's agent output for the analyze-stack step.
const STUB_AGENT_OUTPUT = "Stub backend response";

Cypress.on("uncaught:exception", () => false);

describe("PR-Stack chat", () => {
  it("shows the agent's response after sending a feature request", () => {
    cy.task<{ baseUrl: string; projectId: string; toolPath: string }>("startDaemonForPrStack").then(
      (daemon) => {
        cy.task<string>("getTestSessionToken", { baseUrl: daemon.baseUrl }).then((sessionToken) => {
          cy.task<string>("startPrStackSession", {
            baseUrl: daemon.baseUrl,
            sessionToken,
            projectId: daemon.projectId,
            toolPath: daemon.toolPath,
          }).then((sessionId) => {
            cy.visit(`${daemon.baseUrl}/#/sessions/${sessionId}`, {
              onBeforeLoad(win) {
                win.localStorage.setItem(SESSION_TOKEN_KEY, sessionToken);
              },
            });
            prStackScreenPage.screen().should("exist");
            prStackScreenPage.chatStatus({ timeout: 30000 }).should("contain.text", "Connected");
            prStackScreenPage.sendChatMessage("hi");
            prStackScreenPage
              .chatMessages({ timeout: 30000 })
              .should("contain.text", STUB_AGENT_OUTPUT);
          });
        });
      },
    );
  });
});
