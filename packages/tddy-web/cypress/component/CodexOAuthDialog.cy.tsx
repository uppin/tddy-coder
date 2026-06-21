import React, { useState } from "react";
import { CodexOAuthDialog } from "../../src/components/CodexOAuthDialog";
import { byTestId, TEST_IDS } from "../support/testIds";

describe("CodexOAuthDialog", () => {
  it("opens when an authorize URL is pushed and closes on dismiss", () => {
    // Given — a harness that simulates an RPC push of the authorize URL
    const onDismiss = cy.stub().as("onDismiss");

    function Harness() {
      const [open, setOpen] = useState(false);
      const [authorizeUrl, setAuthorizeUrl] = useState<string | null>(null);

      return (
        <div>
          <button
            type="button"
            data-testid="stub-rpc-push-authorize-url"
            onClick={() => {
              setAuthorizeUrl("https://auth.openai.com/oauth/authorize?client_id=test&state=s");
              setOpen(true);
            }}
          >
            simulate RPC: CodexOAuthUrl
          </button>
          <CodexOAuthDialog
            authorizeUrl={authorizeUrl}
            open={open}
            onDismiss={() => {
              setOpen(false);
              onDismiss();
            }}
          />
        </div>
      );
    }

    cy.mount(<Harness />);

    // When
    byTestId("stub-rpc-push-authorize-url").click();

    // Then — dialog is visible
    byTestId(TEST_IDS.codexOauthDialog, { timeout: 8000 }).should("be.visible");

    // When — user dismisses
    byTestId(TEST_IDS.codexOauthDismiss).click();

    // Then — dialog closes and callback fires
    byTestId(TEST_IDS.codexOauthDialog).should("not.exist");
    cy.get("@onDismiss").should("have.been.calledOnce");
  });
});
