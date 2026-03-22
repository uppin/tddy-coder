/**
 * Acceptance: SessionRuntimeStatusBar story renders a visible status region (E2E against static Storybook).
 * LiveKit + TddyRemote integration is covered by GhosttyTerminalLiveKit runtime + component tests.
 */
describe("Session runtime status (Storybook E2E)", () => {
  it("renders Default story with non-empty session-runtime-status", () => {
    cy.visit(
      "/iframe.html?id=components-sessionruntimestatusbar--default&viewMode=story"
    );
    cy.get("[data-testid='session-runtime-status']", { timeout: 10000 }).should(
      "exist"
    );
    cy.get("[data-testid='session-runtime-status']").should(
      "contain.text",
      "GreenComplete"
    );
    cy.get("[data-testid='session-runtime-status']").should(
      "contain.text",
      "acceptance-tests"
    );
  });
});
