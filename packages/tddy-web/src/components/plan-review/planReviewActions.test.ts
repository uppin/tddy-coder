import { describe, expect, it } from "bun:test";
import {
  expectedApproveMessage,
  expectedRefineMessage,
  expectedRejectAsDismissMessage,
  submitPlanApprove,
  submitPlanRefinement,
  submitPlanReject,
} from "./planReviewActions";

describe("planReviewActions", () => {
  it("submitPlanRefinement invokes onRefineSubmit and emits refine ClientMessage", () => {
    let refineText: string | undefined;
    const onRefineSubmit = (t: string) => {
      refineText = t;
    };
    const messages: unknown[] = [];
    const onClientMessage = (m: unknown) => messages.push(m);

    submitPlanRefinement("hello plan", onRefineSubmit, onClientMessage as (m: import("../../gen/tddy/v1/remote_pb").ClientMessage) => void);

    expect(refineText).toBe("hello plan");
    expect(messages).toHaveLength(1);
    const last = messages[0] as import("../../gen/tddy/v1/remote_pb").ClientMessage;
    expect(last.intent.case).toBe(expectedRefineMessage().intent.case);
  });

  it("submitPlanApprove emits approve ClientMessage", () => {
    const messages: unknown[] = [];
    submitPlanApprove((m) => messages.push(m));

    expect(messages).toHaveLength(1);
    const last = messages[0] as import("../../gen/tddy/v1/remote_pb").ClientMessage;
    expect(last.intent.case).toBe(expectedApproveMessage().intent.case);
  });

  it("submitPlanReject emits dismissViewer ClientMessage (reject semantics)", () => {
    const messages: unknown[] = [];
    submitPlanReject((m) => messages.push(m));

    expect(messages).toHaveLength(1);
    const last = messages[0] as import("../../gen/tddy/v1/remote_pb").ClientMessage;
    expect(last.intent.case).toBe(expectedRejectAsDismissMessage().intent.case);
  });
});
