import React from "react";
import { PlanReviewActivityPane } from "../../src/components/plan-review/PlanReviewActivityPane";
import type { ClientMessage } from "../../src/gen/tddy/v1/remote_pb";
import {
  expectedApproveMessage,
  expectedRefineMessage,
  expectedRejectAsDismissMessage,
} from "../../src/components/plan-review/planReviewClientMessageFixtures";

const SAMPLE_PRD = `# Plan\n\nBody line.\n\n## Section\n\nMore content.`;

/**
 * Reject semantics (PRD acceptance #6): until `reject_plan` exists in protobuf,
 * Reject MUST emit `ClientMessage` with `intent.case === "dismissViewer"`.
 * Update when stakeholders lock semantics and proto is updated.
 */

function assertClientMessageEqual(a: ClientMessage, b: ClientMessage) {
  expect(a.intent.case).to.eq(b.intent.case);
}

function stubCalls(stub: unknown): Array<{ args: [ClientMessage] }> {
  return (stub as { getCalls: () => Array<{ args: [ClientMessage] }> }).getCalls();
}

/** Most recent emitted ClientMessage whose intent matches `caseName` (search newest-first). */
function lastClientMessageMatching(
  stub: unknown,
  caseName: ClientMessage["intent"]["case"],
): ClientMessage | undefined {
  const messages = stubCalls(stub).map((c) => c.args[0]);
  for (let i = messages.length - 1; i >= 0; i--) {
    if (messages[i].intent.case === caseName) {
      return messages[i];
    }
  }
  return undefined;
}

function mountPane(overrides?: {
  onClientMessage?: (m: ClientMessage) => void;
  onRefineSubmit?: (t: string) => void;
  onApprove?: () => void;
  onReject?: () => void;
}) {
  const onClientMessage = cy.stub().as("onClientMessage");
  const onRefineSubmit = cy.stub().as("onRefineSubmit");
  const onApprove = cy.stub().as("onApprove");
  const onReject = cy.stub().as("onReject");
  cy.mount(
    <PlanReviewActivityPane
      prdMarkdown={SAMPLE_PRD}
      onClientMessage={overrides?.onClientMessage ?? onClientMessage}
      onRefineSubmit={overrides?.onRefineSubmit ?? onRefineSubmit}
      onApprove={overrides?.onApprove ?? onApprove}
      onReject={overrides?.onReject ?? onReject}
    />,
  );
  return { onClientMessage, onRefineSubmit, onApprove, onReject };
}

describe("PlanReviewActivityPane (acceptance)", () => {
  it("plan_pane_renders_beside_terminal_not_fullscreen", () => {
    cy.viewport(960, 720);
    mountPane();
    cy.get("[data-testid='session-split-layout']", { timeout: 4000 }).should("exist");
    cy.get("[data-testid='terminal-canvas-region']").then(($term) => {
      const tr = $term[0].getBoundingClientRect();
      const vw = Cypress.config("viewportWidth") ?? 960;
      const vh = Cypress.config("viewportHeight") ?? 720;
      expect(tr.width, "terminal visible width").to.be.greaterThan(80);
      expect(tr.height, "terminal visible height").to.be.greaterThan(80);
      expect(tr.width, "terminal not full viewport width").to.be.lessThan(vw * 0.92);
      expect(tr.height, "terminal not full viewport height").to.be.lessThan(vh * 0.92);
    });
    cy.get("[data-testid='plan-activity-pane']").then(($pane) => {
      const pr = $pane[0].getBoundingClientRect();
      const vw = Cypress.config("viewportWidth") ?? 960;
      const vh = Cypress.config("viewportHeight") ?? 720;
      expect(pr.width, "plan pane bounded width").to.be.greaterThan(80).and.lessThan(vw * 0.92);
      expect(pr.height, "plan pane bounded height").to.be.greaterThan(80).and.lessThan(vh * 0.92);
    });
    cy.get("[data-testid='plan-activity-pane']").should(($el) => {
      const pos = window.getComputedStyle($el[0]).position;
      expect(pos).not.to.eq("fixed");
    });
    cy.get("[data-testid='terminal-canvas-region']").then(($t) => {
      cy.get("[data-testid='plan-activity-pane']").then(($p) => {
        const tr = $t[0].getBoundingClientRect();
        const pr = $p[0].getBoundingClientRect();
        const horizontalSplit = tr.right <= pr.left + 2 || pr.right <= tr.left + 2;
        expect(horizontalSplit, "terminal and plan are side-by-side columns").to.be.true;
      });
    });
  });

  it("plan_pane_refinement_prompt_submittable_while_open", () => {
    mountPane();
    cy.get("[data-testid='plan-refine-submit']").should("be.disabled");
    cy.get("[data-testid='plan-refine-input']").should("be.visible").type("Add section on testing");
    cy.get("[data-testid='plan-refine-submit']").should("not.be.disabled");
    cy.get("[data-testid='plan-refine-submit']").click();
    cy.get("@onRefineSubmit").should("have.been.calledOnceWith", "Add section on testing");
    cy.get("@onClientMessage").should("have.been.called");
    cy.get("@onClientMessage").then((stub) => {
      const refine = lastClientMessageMatching(stub, "refinePlan");
      expect(refine, "RefinePlan client message emitted").to.be.ok;
      assertClientMessageEqual(refine!, expectedRefineMessage());
    });
  });

  it("approve_reject_visible_at_end_focusable_and_clickable", () => {
    mountPane();
    cy.get("[data-testid='plan-markdown-scroll']").should("exist");
    cy.get("[data-testid='plan-markdown-scroll']").then(($scroll) => {
      cy.get("[data-testid='plan-action-footer']").then(($foot) => {
        const pos = $scroll[0].compareDocumentPosition($foot[0]);
        expect(
          pos & Node.DOCUMENT_POSITION_FOLLOWING,
          "footer after markdown scroll region in document order",
        ).to.be.greaterThan(0);
      });
    });
    cy.get("[data-testid='plan-action-footer']")
      .find("[data-testid='plan-approve-button']")
      .should("be.visible");
    cy.get("[data-testid='plan-action-footer']")
      .find("[data-testid='plan-reject-button']")
      .should("be.visible");
    cy.get("[data-testid='plan-approve-button']").focus();
    cy.focused().should("have.attr", "data-testid", "plan-approve-button");
    cy.get("[data-testid='plan-reject-button']").focus();
    cy.focused().should("have.attr", "data-testid", "plan-reject-button");
    cy.get("[data-testid='plan-approve-button']").click({ force: false });
    cy.get("@onClientMessage").should("have.been.called");
    cy.get("@onClientMessage").then((stub) => {
      const approveCall = lastClientMessageMatching(stub, "approvePlan");
      expect(approveCall).to.be.ok;
      assertClientMessageEqual(approveCall!, expectedApproveMessage());
    });
    cy.get("[data-testid='plan-reject-button']").click({ force: false });
    cy.get("@onClientMessage").then((stub) => {
      const rejectCall = lastClientMessageMatching(stub, "dismissViewer");
      expect(rejectCall).to.be.ok;
      assertClientMessageEqual(rejectCall!, expectedRejectAsDismissMessage());
    });
  });

  it("keyboard_can_activate_approve_and_reject_without_mouse", () => {
    mountPane();
    cy.get("[data-testid='plan-approve-button']").focus().type("{enter}");
    cy.get("@onClientMessage").should("have.been.called");
    cy.get("@onClientMessage").then((stub) => {
      const approveCall = lastClientMessageMatching(stub, "approvePlan");
      expect(approveCall).to.be.ok;
    });
    cy.get("@onClientMessage").invoke("resetHistory");
    cy.get("[data-testid='plan-reject-button']").focus().type(" ");
    cy.get("@onClientMessage").should("have.been.called");
    cy.get("@onClientMessage").then((stub) => {
      const rejectCall = lastClientMessageMatching(stub, "dismissViewer");
      expect(rejectCall).to.be.ok;
    });
  });
});
