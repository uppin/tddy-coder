/**
 * The `Toast` UI component: a message, at most one action, and the action's error.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-02-dormant-agent-tab.md (`#session-tabs 6/7`)
 */

import React from "react";
import { Toast } from "../../src/components/ui/toast";
import { byTestId } from "../support/testIds";

const TOAST = "a-toast";

describe("Toast", () => {
  it("shows its message and its action", () => {
    cy.mount(<Toast testId={TOAST} message="This session is not running" action={{ label: "Resume", onClick: cy.stub() }} />);

    byTestId(TOAST).should("have.attr", "role", "status").and("contain.text", "This session is not running");
    byTestId(`${TOAST}-action`).should("be.visible").and("contain.text", "Resume");
  });

  it("fires its action once per press", () => {
    const onClick = cy.stub().as("onClick");
    cy.mount(<Toast testId={TOAST} message="Not running" action={{ label: "Resume", onClick }} />);

    byTestId(`${TOAST}-action`).click();

    cy.get("@onClick").should("have.been.calledOnce");
  });

  it("cannot be pressed while its action is busy", () => {
    cy.mount(
      <Toast testId={TOAST} message="Not running" action={{ label: "Resume", onClick: cy.stub(), busy: true }} />,
    );

    byTestId(`${TOAST}-action`).should("be.disabled");
  });

  it("shows its action's error as an alert", () => {
    cy.mount(
      <Toast
        testId={TOAST}
        message="Not running"
        action={{ label: "Resume", onClick: cy.stub() }}
        error="the daemon refused the resume"
      />,
    );

    byTestId(`${TOAST}-error`).should("have.attr", "role", "alert").and("contain.text", "the daemon refused the resume");
  });
});
