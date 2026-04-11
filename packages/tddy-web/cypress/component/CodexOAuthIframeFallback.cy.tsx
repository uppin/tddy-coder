import React from "react";
import { CodexOAuthDialog } from "../../src/components/CodexOAuthDialog";

describe("Codex OAuth iframe-blocked provider path", () => {
  it("shows documented top-level or popup fallback when embedding is blocked", () => {
    cy.mount(
      <CodexOAuthDialog
        authorizeUrl="https://auth.openai.com/oauth/authorize?state=blocked"
        open={true}
        onDismiss={() => {}}
        embeddingBlocked={true}
      />,
    );

    cy.get("[data-testid='codex-oauth-embedding-fallback']", { timeout: 8000 }).should(
      "be.visible",
    );
  });
});
