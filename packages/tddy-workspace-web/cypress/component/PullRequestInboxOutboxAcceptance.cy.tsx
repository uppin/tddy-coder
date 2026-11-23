/**
 * Acceptance (`#workqueue` 11/14): the PR view of a work item shows its Inbox (sources and conflict
 * markers, trunk first) and its Outbox (per successor).
 *
 * Feature: docs/ft/coder/pr-stacking.md § Commit inbox and outbox
 */

import React from "react";
import { PullRequestInboxTab, PullRequestOutboxTab } from "../../src/pullRequests/PullRequestInboxTab";

const byTestId = (id: string) => cy.get(`[data-testid="${id}"]`, { timeout: 5000 });

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("the inbox lists each commit with its source and a conflict marker naming the files", () => {
  // When
  cy.mount(
    <PullRequestInboxTab
      inbox={[
        { sha: "t1", subject: "trunk moved", source: "trunk:master", conflictFiles: [] },
        { sha: "x1", subject: "fix one", source: "item:n1", conflictFiles: ["f.txt"] },
      ]}
    />,
  );

  // Then
  byTestId("pr-inbox-commit-t1").should("have.attr", "data-source", "trunk:master").and("have.attr", "data-conflict", "false");
  byTestId("pr-inbox-commit-x1").should("have.attr", "data-conflict", "true");
  byTestId("pr-inbox-conflict-files-x1").should("contain.text", "f.txt");
});

it("the outbox lists, per successor, what it lacks", () => {
  // When
  cy.mount(<PullRequestOutboxTab outbox={[{ successor: "n2", commits: [{ sha: "x1", subject: "fix one" }] }]} />);

  // Then
  byTestId("pr-outbox-successor-n2").should("be.visible");
  byTestId("pr-outbox-commit-n2-x1").should("contain.text", "fix one");
});
