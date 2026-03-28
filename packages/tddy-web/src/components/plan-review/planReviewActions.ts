import { create } from "@bufbuild/protobuf";
import type { ClientMessage } from "../../gen/tddy/v1/remote_pb";
import {
  ApprovePlanSchema,
  ClientMessageSchema,
  DismissViewerSchema,
  RefinePlanSchema,
} from "../../gen/tddy/v1/remote_pb";
import { logPlanReviewMarker } from "./planReviewMarkers";

function devDebug(...args: Parameters<typeof console.debug>): void {
  if (import.meta.env.DEV) {
    console.debug(...args);
  }
}

function devInfo(...args: Parameters<typeof console.info>): void {
  if (import.meta.env.DEV) {
    console.info(...args);
  }
}

export function submitPlanRefinement(
  text: string,
  onRefineSubmit: (text: string) => void,
  onClientMessage?: (message: ClientMessage) => void,
): void {
  logPlanReviewMarker("M002", "plan_review::submitPlanRefinement", {
    textLen: text.length,
  });
  devDebug("[plan-review] submitPlanRefinement", {
    textLength: text.length,
    hasTransport: Boolean(onClientMessage),
  });

  onRefineSubmit(text);

  const msg = expectedRefineMessage();
  onClientMessage?.(msg);
  devInfo("[plan-review] RefinePlan ClientMessage emitted", {
    intentCase: msg.intent.case,
  });
}

export function submitPlanApprove(onClientMessage?: (message: ClientMessage) => void): void {
  logPlanReviewMarker("M003", "plan_review::submitPlanApprove", {});
  devDebug("[plan-review] submitPlanApprove", { hasTransport: Boolean(onClientMessage) });

  const msg = expectedApproveMessage();
  onClientMessage?.(msg);
  devInfo("[plan-review] ApprovePlan ClientMessage emitted", {
    intentCase: msg.intent.case,
  });
}

/** Reject uses dismissViewer until a dedicated reject intent exists in protobuf. */
export function submitPlanReject(onClientMessage?: (message: ClientMessage) => void): void {
  logPlanReviewMarker("M004", "plan_review::submitPlanReject", {});
  devDebug("[plan-review] submitPlanReject", { hasTransport: Boolean(onClientMessage) });

  const msg = expectedRejectAsDismissMessage();
  onClientMessage?.(msg);
  devInfo("[plan-review] dismissViewer ClientMessage emitted (reject semantics)", {
    intentCase: msg.intent.case,
  });
}

export function expectedApproveMessage(): ClientMessage {
  return create(ClientMessageSchema, {
    intent: { case: "approvePlan", value: create(ApprovePlanSchema, {}) },
  });
}

export function expectedRefineMessage(): ClientMessage {
  return create(ClientMessageSchema, {
    intent: { case: "refinePlan", value: create(RefinePlanSchema, {}) },
  });
}

export function expectedRejectAsDismissMessage(): ClientMessage {
  return create(ClientMessageSchema, {
    intent: { case: "dismissViewer", value: create(DismissViewerSchema, {}) },
  });
}
