import React from "react";
import { SessionRuntimeStatusBar } from "../../src/components/SessionRuntimeStatusBar";

describe("SessionRuntimeStatusBar", () => {
  it("renders status_line text in the session region", () => {
    const expected =
      "Goal: test-goal | Running | 42s | stub-agent | stub-model — acceptance snapshot";
    cy.mount(
      <div style={{ width: 400 }}>
        <SessionRuntimeStatusBar statusLine={expected} />
      </div>
    );
    cy.get("[data-testid='session-runtime-status']", { timeout: 5000 }).should(
      "exist"
    );
    cy.get("[data-testid='session-runtime-status']").should(
      "contain.text",
      expected
    );
  });

  it("updates when statusLine prop changes", () => {
    const Wrapper = () => {
      const [line, setLine] = React.useState("First snapshot");
      return (
        <div style={{ width: 400 }}>
          <SessionRuntimeStatusBar statusLine={line} />
          <button
            type="button"
            data-testid="advance-status"
            onClick={() =>
              setLine("Second snapshot after workflow transition")
            }
          >
            advance
          </button>
        </div>
      );
    };
    cy.mount(<Wrapper />);
    cy.get("[data-testid='session-runtime-status']").should(
      "contain.text",
      "First snapshot"
    );
    cy.get("[data-testid='advance-status']").click();
    cy.get("[data-testid='session-runtime-status']").should(
      "contain.text",
      "Second snapshot after workflow transition"
    );
  });
});
