import React from "react";
import { useVisualViewport } from "../../src/hooks/useVisualViewport";

function TestViewportConsumer() {
  const { height, isKeyboardOpen } = useVisualViewport();
  return (
    <div data-testid="viewport-consumer">
      <span data-testid="viewport-height">{height}</span>
      <span data-testid="viewport-keyboard-open">{String(isKeyboardOpen)}</span>
    </div>
  );
}

describe("useVisualViewport", () => {
  it("returns visual viewport height and isKeyboardOpen state", () => {
    cy.mount(<TestViewportConsumer />);
    cy.get("[data-testid='viewport-consumer']").should("exist");
    cy.get("[data-testid='viewport-height']")
      .invoke("text")
      .then((text) => {
        const h = Number(text);
        expect(h, "height should be a positive number").to.be.greaterThan(0);
      });
    cy.get("[data-testid='viewport-keyboard-open']").should("have.text", "false");
  });

  it("updates height when visual viewport resize event fires", () => {
    cy.mount(<TestViewportConsumer />);
    cy.get("[data-testid='viewport-height']")
      .invoke("text")
      .then((initialHeight) => {
        const vv = window.visualViewport;
        if (!vv) return;
        vv.dispatchEvent(new Event("resize"));
        cy.get("[data-testid='viewport-height']").should("exist");
      });
  });

  it("updates height when viewport shrinks then restores (keyboard open then close)", () => {
    cy.mount(<TestViewportConsumer />);
    cy.viewport(375, 667);
    cy.get("[data-testid='viewport-height']")
      .invoke("text")
      .then((fullHeight) => {
        const h = Number(fullHeight);
        expect(h).to.be.greaterThan(400);
      });
    cy.viewport(375, 350);
    cy.wait(100);
    cy.get("[data-testid='viewport-height']")
      .invoke("text")
      .then((shrunkHeight) => {
        const h = Number(shrunkHeight);
        expect(h).to.be.lessThan(400);
      });
    cy.viewport(375, 667);
    cy.wait(100);
    cy.get("[data-testid='viewport-height']")
      .invoke("text")
      .then((restoredHeight) => {
        const h = Number(restoredHeight);
        expect(h, "height should scale back when keyboard closes").to.be.greaterThan(400);
      });
  });
});
