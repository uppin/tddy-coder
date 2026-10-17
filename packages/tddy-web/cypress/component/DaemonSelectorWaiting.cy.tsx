/**
 * `DaemonSelector` on its own: how it shows a selected daemon that is not listed right now.
 *
 * PRD: docs/ft/web/daemon-selector-livekit-rpc.md.
 */

import React from "react";
import { DaemonSelector } from "../../src/components/shell/DaemonSelector";
import type { DaemonHost } from "../../src/lib/participantRole";
import { daemonSelectorPage } from "../support/pages/daemonSelectorPage";

const UDOO: DaemonHost = { instanceId: "udoo", label: "udoo (this daemon)" };
const LAPTOP_B: DaemonHost = { instanceId: "laptop-b", label: "Laptop B" };

function mountSelector(props: {
  daemons: DaemonHost[];
  selectedInstanceId: string;
  selectedLabel: string | null;
}) {
  cy.mount(
    <DaemonSelector
      daemons={props.daemons}
      selectedInstanceId={props.selectedInstanceId}
      servingInstanceId="udoo"
      selectedLabel={props.selectedLabel}
      onSelect={cy.stub().as("onSelect")}
    />,
  );
}

/** Asserts the selector never reported a choice to `mountSelector`'s `onSelect`. */
function expectNothingChosen() {
  cy.get("@onSelect").should("not.have.been.called");
}

it("shows no waiting dot while the selected daemon is listed", () => {
  // Given / When
  mountSelector({ daemons: [UDOO, LAPTOP_B], selectedInstanceId: "laptop-b", selectedLabel: "Laptop B" });

  // Then
  daemonSelectorPage.expectShowsSelectedExactly("Laptop B");
  daemonSelectorPage.expectNotWaiting();
});

it("shows an unlisted selected daemon under its remembered label, waiting", () => {
  // Given / When
  mountSelector({ daemons: [UDOO], selectedInstanceId: "laptop-b", selectedLabel: "Laptop B" });

  // Then
  daemonSelectorPage.expectWaitingFor("Laptop B");
});

it("strips the self-label suffix from another daemon's remembered label", () => {
  // Given / When — laptop-b advertised itself, from its own perspective, as "(this daemon)"
  mountSelector({
    daemons: [UDOO],
    selectedInstanceId: "laptop-b",
    selectedLabel: "laptop-b (this daemon)",
  });

  // Then
  daemonSelectorPage.expectShowsSelectedExactly("laptop-b");
});

it("lists the waiting daemon as an option that cannot be chosen", () => {
  // Given
  mountSelector({ daemons: [UDOO], selectedInstanceId: "laptop-b", selectedLabel: "Laptop B" });

  // When — the operator clicks it anyway
  daemonSelectorPage.forceChoose("laptop-b");

  // Then — it is listed as unavailable, and clicking it chose nothing
  daemonSelectorPage.expectOptionUnavailable("laptop-b");
  expectNothingChosen();
});
