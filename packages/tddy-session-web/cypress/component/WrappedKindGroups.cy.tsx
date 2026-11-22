/**
 * Component: the "+" menu's wrapped and unwrapped groups — the same kinds twice, the first group's
 * each opening in its wrapper, the second's unwrapped, every item's test id the group's prefix and
 * its kind (`#tab-hardening` 2/4). The sandbox and target groupings of `NewTerminalMenu` are this
 * one component; `NewTerminalMenu{Sandbox,Target}Acceptance` cover it inside the menu.
 *
 * Pure UI: `onChoose` is the whole of its contract with the menu, so nothing here makes an RPC.
 */

import React from "react";
import { WrappedKindGroups } from "../../src/WrappedKindGroups";

const KINDS = ["bash", "muse-cli"] as const;

/** Every choice the component reported, in order, as `{ kind, wrappers }`. */
type Choice = { kind: string; wrappers: readonly string[] | undefined };

function mountTheSandboxGrouping(choices: Choice[]) {
  cy.mount(
    <div role="menu">
      <WrappedKindGroups
        kinds={KINDS}
        wrapped={{
          label: "In sandbox",
          testIdPrefix: "sessions-terminal-tab-new-in-sandbox",
          wrapper: "sandbox",
        }}
        unwrapped={{ label: "On host", testIdPrefix: "sessions-terminal-tab-new-on-host" }}
        onChoose={(kind, wrappers) => choices.push({ kind, wrappers })}
      />
    </div>,
  );
}

/** The aria-label of each group, in order. */
function groupLabels() {
  return cy
    .get('[role="group"]')
    .then(($groups) => $groups.toArray().map((group) => group.getAttribute("aria-label")));
}

/** The test id of each item of the group labelled `label`, in order. */
function itemTestIdsOf(label: string) {
  return cy
    .get(`[role="group"][aria-label="${label}"] [role="menuitem"]`)
    .then(($items) => $items.toArray().map((item) => item.getAttribute("data-testid")));
}

describe("WrappedKindGroups", () => {
  it("renders the wrapped group first, then the unwrapped one", () => {
    // Given / When
    mountTheSandboxGrouping([]);

    // Then
    groupLabels().should("deep.equal", ["In sandbox", "On host"]);
  });

  it("names every item by its group's test-id prefix and its kind", () => {
    // Given / When
    mountTheSandboxGrouping([]);

    // Then
    itemTestIdsOf("In sandbox").should("deep.equal", [
      "sessions-terminal-tab-new-in-sandbox-bash",
      "sessions-terminal-tab-new-in-sandbox-muse-cli",
    ]);
    itemTestIdsOf("On host").should("deep.equal", [
      "sessions-terminal-tab-new-on-host-bash",
      "sessions-terminal-tab-new-on-host-muse-cli",
    ]);
  });

  it("marks the wrapped group as the menu's default list", () => {
    // Given / When
    mountTheSandboxGrouping([]);

    // Then
    cy.get('[data-testid="sessions-terminal-tab-new-menu"]').should(
      "have.attr",
      "aria-label",
      "In sandbox",
    );
  });

  it("reports a wrapped choice with its wrapper and an unwrapped choice with none", () => {
    // Given
    const choices: Choice[] = [];
    mountTheSandboxGrouping(choices);

    // When
    cy.get('[data-testid="sessions-terminal-tab-new-in-sandbox-muse-cli"]').click();
    cy.get('[data-testid="sessions-terminal-tab-new-on-host-bash"]').click();

    // Then
    cy.wrap(choices).should("deep.equal", [
      { kind: "muse-cli", wrappers: ["sandbox"] },
      { kind: "bash", wrappers: undefined },
    ]);
  });
});
