import React from "react";
import { SessionSearchInput } from "../../src/components/session/SessionSearchInput";

describe("SessionSearchInput", () => {
  it("web_search_input_triggers_query_debounced", () => {
    const onSearch = cy.stub().as("onSearch");

    cy.mount(
      <SessionSearchInput debounceMs={300} onSearchQuery={onSearch} />,
    );

    cy.get("@onSearch").invoke("resetHistory");
    cy.get('[data-testid="session-search-input"]').type("auth");
    cy.get("@onSearch").should("not.have.been.called");
    cy.wait(350);
    cy.get("@onSearch").should("have.been.calledOnceWith", "auth");
  });
});
