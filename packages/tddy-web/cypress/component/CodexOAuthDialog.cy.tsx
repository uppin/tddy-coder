import React, { useState } from "react";
import { CodexOAuthDialog } from "../../src/components/CodexOAuthDialog";

describe("Codex OAuth web relay dialog", () => {
  it("opens and closes on stub RPC events (authorize URL then dismiss)", () => {
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
    cy.get("[data-testid='stub-rpc-push-authorize-url']").click();
    cy.get("[data-testid='codex-oauth-dialog']", { timeout: 8000 }).should("be.visible");
    cy.get("[data-testid='codex-oauth-dismiss']").click();
    cy.get("[data-testid='codex-oauth-dialog']").should("not.exist");
    cy.get("@onDismiss").should("have.been.calledOnce");
  });
});
