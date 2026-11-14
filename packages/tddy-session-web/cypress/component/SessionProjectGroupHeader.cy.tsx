/**
 * A project group's header row (`#orchestrators` 8/11): the collapse toggle, then the "+" that opens
 * the orchestrator composer, then a slot for further actions. The "+" is a sibling of the toggle,
 * so pressing it never collapses the group, and a header given no `onNew` shows none.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-10-orchestrator-composer.md
 */

import React from "react";
import { SessionProjectGroupHeader } from "../../src/SessionProjectGroupHeader";
import type { ProjectSessionGroup } from "../../src/utils/sessionProjectGroups";
import {
  byTestId,
  sessionsDrawerProjectGroupHeader,
  sessionsDrawerProjectNew,
} from "../support/testIds";

const BETA = "proj-beta";
const BETA_GROUP: ProjectSessionGroup = { projectId: BETA, label: "beta", sessions: [] };

function aHeader({ onNew, actions }: { onNew?: () => void; actions?: React.ReactNode } = {}) {
  const onToggle = cy.stub().as("onToggle");
  cy.mount(
    <SessionProjectGroupHeader
      group={BETA_GROUP}
      groupKey={BETA}
      collapsed={false}
      onToggle={onToggle}
      onNew={onNew}
      actions={actions}
    />,
  );
}

it("pressing the plus opens the composer and leaves the group as it was", () => {
  // Given
  const onNew = cy.stub().as("onNew");
  aHeader({ onNew });

  // When
  byTestId(sessionsDrawerProjectNew(BETA)).click();

  // Then
  cy.get("@onNew").should("have.been.calledOnce");
  cy.get("@onToggle").should("not.have.been.called");
});

it("the toggle still collapses the group, and is not the plus", () => {
  // Given
  aHeader({ onNew: cy.stub().as("onNew") });

  // When
  byTestId(sessionsDrawerProjectGroupHeader(BETA)).click();

  // Then
  cy.get("@onToggle").should("have.been.calledOnce");
  cy.get("@onNew").should("not.have.been.called");
  byTestId(sessionsDrawerProjectGroupHeader(BETA)).find(`[data-testid='${sessionsDrawerProjectNew(BETA)}']`).should("not.exist");
});

it("renders the actions slot after the plus", () => {
  // Given
  const gear = <button type="button" data-testid="a-header-action">gear</button>;

  // When
  aHeader({ onNew: cy.stub(), actions: gear });

  // Then
  byTestId(sessionsDrawerProjectNew(BETA)).should("be.visible");
  byTestId("a-header-action").should("be.visible");
});

it("a header given no onNew shows no plus", () => {
  // When
  aHeader();

  // Then
  byTestId(sessionsDrawerProjectGroupHeader(BETA)).should("be.visible");
  byTestId(sessionsDrawerProjectNew(BETA)).should("not.exist");
});
