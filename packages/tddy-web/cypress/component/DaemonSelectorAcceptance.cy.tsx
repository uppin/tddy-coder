/**
 * Acceptance tests: the top-right `DaemonSelector` lists the common-room daemon-role
 * participants. Every daemon's own LiveKit advertisement self-labels itself
 * `"{id} (this daemon)"` from its own perspective, so that substring cannot be trusted as "this is
 * the daemon serving this web session" — the selector must resolve that against the configured
 * `servingInstanceId` (from `/api/config`) and strip the self-referential suffix from every other
 * entry.
 *
 * PRD: docs/ft/web/daemon-selector-livekit-rpc.md.
 */

import React from "react";
import { DaemonSelector } from "../../src/components/shell/DaemonSelector";
import type { DaemonHost } from "../../src/lib/participantRole";
import { daemonSelectorPage } from "../support/pages/daemonSelectorPage";

const UDOO: DaemonHost = { instanceId: "udoo", label: "udoo (this daemon)" };
const LAPTOP_B: DaemonHost = { instanceId: "laptop-b", label: "laptop-b (this daemon)" };

it("marks only the serving daemon as '(this daemon)', stripping the self-referential label from peers", () => {
  // Given — two daemons, both self-labeling "(this daemon)"; udoo is the one serving this session
  cy.mount(
    <DaemonSelector
      daemons={[UDOO, LAPTOP_B]}
      selectedInstanceId="udoo"
      servingInstanceId="udoo"
      onSelect={cy.stub()}
    />,
  );

  // When / Then
  daemonSelectorPage.optionLabels().should("deep.equal", ["udoo (this daemon)", "laptop-b"]);
});

it("selecting a peer daemon from the dropdown calls onSelect with its instance id", () => {
  // Given
  const onSelect = cy.stub().as("onSelect");
  cy.mount(
    <DaemonSelector
      daemons={[UDOO, LAPTOP_B]}
      selectedInstanceId="udoo"
      servingInstanceId="udoo"
      onSelect={onSelect}
    />,
  );

  // When
  daemonSelectorPage.choose("laptop-b");

  // Then
  cy.get("@onSelect").should("have.been.calledWith", "laptop-b");
});
