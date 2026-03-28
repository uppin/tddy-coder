import type { Meta, StoryObj } from "@storybook/react";
import {
  PlanReviewActivityPane,
  type PlanReviewActivityPaneProps,
} from "./PlanReviewActivityPane";
import {
  expectedApproveMessage,
  expectedRefineMessage,
  expectedRejectAsDismissMessage,
} from "./planReviewClientMessageFixtures";

const meta: Meta<typeof PlanReviewActivityPane> = {
  title: "Session/PlanReviewActivityPane",
  component: PlanReviewActivityPane,
};

export default meta;

type Story = StoryObj<typeof PlanReviewActivityPane>;

function mkHandlers(overrides?: Partial<PlanReviewActivityPaneProps>): PlanReviewActivityPaneProps {
  return {
    prdMarkdown: "# PRD\n\nExample plan body.",
    onRefineSubmit: () => {},
    onApprove: () => {},
    onReject: () => {},
    onClientMessage: () => {},
    ...overrides,
  };
}

/** Plan-review activity pane beside the terminal region (split layout). */
export const PlanReviewStub: Story = {
  args: mkHandlers({
    onClientMessage: (msg) => {
      void msg;
    },
  }),
};

export const PlanReviewWithClientMessageLogging: Story = {
  args: mkHandlers({
    onClientMessage: (msg) => {
      if (typeof console !== "undefined" && console.debug) {
        console.debug("ClientMessage", msg.intent.case);
      }
    },
  }),
};

/** Same shapes as `planReviewActions` expected* helpers — for Cypress and manual verification. */
export const ExampleClientMessages = {
  approve: expectedApproveMessage(),
  refine: expectedRefineMessage(),
  rejectDismissViewer: expectedRejectAsDismissMessage(),
};
