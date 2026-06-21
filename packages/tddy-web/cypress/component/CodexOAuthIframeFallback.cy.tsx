import React from "react";
import { CodexOAuthDialog } from "../../src/components/CodexOAuthDialog";
import { byTestId, TEST_IDS } from "../support/testIds";

describe("CodexOAuthDialog — iframe-blocked fallback", () => {
  it("shows the top-level/popup fallback when embedding is blocked by the provider", () => {
    // Given
    cy.mount(
      <CodexOAuthDialog
        authorizeUrl="https://auth.openai.com/oauth/authorize?state=blocked"
        open={true}
        onDismiss={() => {}}
        embeddingBlocked={true}
      />,
    );

    // Then
    byTestId(TEST_IDS.codexOauthEmbeddingFallback, { timeout: 8000 }).should("be.visible");
  });
});
