/**
 * The shell's RPC Playground menu entry — kept with `tddy-web` when the playground screen moved
 * to `tddy-rpc-playground-web`: the menu is the shell's, so its spec stays here.
 */

import React from "react";
import { byTestId, TEST_IDS } from "../support/testIds";
import { DaemonNavMenu } from "../../src/components/shell/DaemonNavMenu";

describe("DaemonNavMenu — RPC Playground entry", () => {
  it("renders shell-menu-rpc-playground menu item", () => {
    // Given
    cy.mount(<DaemonNavMenu onNavigate={cy.stub().as("onNavigate")} />);

    // When
    byTestId(TEST_IDS.shellMenuButton).click();

    // Then
    byTestId(TEST_IDS.shellMenuRpcPlayground).should("be.visible");
  });

  it("clicking shell-menu-rpc-playground navigates to /rpc-playground", () => {
    // Given
    cy.mount(<DaemonNavMenu onNavigate={cy.stub().as("onNavigate")} />);
    byTestId(TEST_IDS.shellMenuButton).click();

    // When
    byTestId(TEST_IDS.shellMenuRpcPlayground).click();

    // Then
    cy.get("@onNavigate").should("have.been.calledWith", "/rpc-playground");
  });
});
