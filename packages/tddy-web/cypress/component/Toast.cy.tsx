/**
 * The `Toast` UI component: a message, at most one action, and the action's error.
 *
 * PRD: docs/ft/web/session-terminal-tabs.md (`#session-tabs 6/7`)
 */

import React from "react";
import { Toast } from "../../src/components/ui/toast";
import { byTestId } from "../support/testIds";

const TOAST = "a-toast";

describe("Toast", () => {
  it("shows its message and its action", () => {
    // Given / When
    cy.mount(<Toast testId={TOAST} message="This session is not running" action={{ label: "Resume", onClick: cy.stub() }} />);

    // Then
    byTestId(TOAST).should("have.attr", "role", "status").and("contain.text", "This session is not running");
    byTestId(`${TOAST}-action`).should("be.visible").and("contain.text", "Resume");
  });

  it("fires its action once per press", () => {
    // Given
    const onClick = cy.stub().as("onClick");
    cy.mount(<Toast testId={TOAST} message="Not running" action={{ label: "Resume", onClick }} />);

    // When
    byTestId(`${TOAST}-action`).click();

    // Then
    cy.get("@onClick").should("have.been.calledOnce");
  });

  it("cannot be pressed while its action is busy", () => {
    // Given / When
    cy.mount(
      <Toast testId={TOAST} message="Not running" action={{ label: "Resume", onClick: cy.stub(), busy: true }} />,
    );

    // Then
    byTestId(`${TOAST}-action`).should("be.disabled");
  });

  it("shows its action's error as an alert", () => {
    // Given / When
    cy.mount(
      <Toast
        testId={TOAST}
        message="Not running"
        action={{ label: "Resume", onClick: cy.stub() }}
        error="the daemon refused the resume"
      />,
    );

    // Then
    byTestId(`${TOAST}-error`).should("have.attr", "role", "alert").and("contain.text", "the daemon refused the resume");
  });
});
