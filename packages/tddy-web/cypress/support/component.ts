import React from "react";
import { mount } from "cypress/react";
import { SessionTablesStoryLayout } from "@/components/connection/ConnectionSessionTablesSection.demo";

Cypress.Commands.add("mount", mount);

Cypress.Commands.add(
  "mountSessionTablesDemo",
  (options: { outerWidthPx: number; viewport?: [number, number] }) => {
    const [vw, vh] = options.viewport ?? [1280, 800];
    cy.viewport(vw, vh);
    cy.mount(
      React.createElement(SessionTablesStoryLayout, { outerWidthPx: options.outerWidthPx }),
    );
    return cy.get('[data-testid="session-tables-layout-host"]', { timeout: 8000 }).should("exist");
  },
);
