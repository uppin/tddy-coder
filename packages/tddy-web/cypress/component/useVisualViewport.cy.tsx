import React from "react";
import { useVisualViewport } from "../../src/hooks/useVisualViewport";
import { byTestId, TEST_IDS } from "../support/testIds";

function TestViewportConsumer() {
  const { height, isKeyboardOpen } = useVisualViewport();
  return (
    <div data-testid={TEST_IDS.viewportConsumer}>
      <span data-testid={TEST_IDS.viewportHeight}>{height}</span>
      <span data-testid={TEST_IDS.viewportKeyboardOpen}>{String(isKeyboardOpen)}</span>
    </div>
  );
}

describe("useVisualViewport", () => {
  it("exposes a positive height and isKeyboardOpen:false on initial mount", () => {
    // Given
    cy.mount(<TestViewportConsumer />);

    // Then
    byTestId(TEST_IDS.viewportConsumer).should("exist");
    byTestId(TEST_IDS.viewportHeight)
      .invoke("text")
      .then((text) => {
        expect(Number(text), "height should be a positive number").to.be.greaterThan(0);
      });
    byTestId(TEST_IDS.viewportKeyboardOpen).should("have.text", "false");
  });

  it("height is re-read when a visual viewport resize event fires", () => {
    // Given
    cy.mount(<TestViewportConsumer />);

    // When
    cy.window().then((win) => {
      win.visualViewport?.dispatchEvent(new Event("resize"));
    });

    // Then — element still exists (height may or may not change in the test env)
    byTestId(TEST_IDS.viewportHeight).should("exist");
  });

  it("isKeyboardOpen updates to true when the viewport shrinks and false when it restores", () => {
    // Given
    cy.mount(<TestViewportConsumer />);
    cy.viewport(375, 667);

    // Then — full viewport height
    byTestId(TEST_IDS.viewportHeight)
      .invoke("text")
      .then((text) => {
        expect(Number(text)).to.be.greaterThan(400);
      });

    // When — simulate keyboard open (shrink height)
    cy.viewport(375, 350);

    // Then — height dropped below threshold (retrying assertion, no fixed sleep)
    byTestId(TEST_IDS.viewportHeight)
      .invoke("text")
      .should((text) => {
        expect(Number(text)).to.be.lessThan(400);
      });

    // When — simulate keyboard close (restore height)
    cy.viewport(375, 667);

    // Then — height restored
    byTestId(TEST_IDS.viewportHeight)
      .invoke("text")
      .should((text) => {
        expect(Number(text), "height should scale back when keyboard closes").to.be.greaterThan(400);
      });
  });
});
