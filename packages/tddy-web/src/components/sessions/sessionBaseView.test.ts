import { describe, it, expect } from "bun:test";
import { canResumeSession, sessionBaseViewMode } from "./sessionBaseView";
import type { SessionEntry } from "../../gen/connection_pb";

/** Named for the `hasWorkflowView` argument, so a call site reads as a sentence rather than as a
 *  bare boolean whose meaning the reader has to look up. */
const WITH_WORKFLOW_VIEW = true;
const WITHOUT_WORKFLOW_VIEW = false;

const aSession = (fields: Partial<SessionEntry>): SessionEntry =>
  ({ isActive: false, pendingElicitation: false, ...fields }) as SessionEntry;

const anActiveSession = () => aSession({ isActive: true });
const anInactiveSession = () => aSession({ isActive: false });
/** A running session waiting on an elicitation — "needs input", which counts as live. */
const aLiveElicitingSession = () => aSession({ isActive: true, pendingElicitation: true });
/** An agent that died mid-elicitation: the flag is persisted and never cleared, so it outlives the
 *  process. Nothing is waiting on the operator, so this is dormant. */
const aDeadElicitingSession = () => aSession({ isActive: false, pendingElicitation: true });

describe("sessionBaseViewMode", () => {
  it("shows the activities view for an inactive session with no workflow view", () => {
    expect(sessionBaseViewMode(anInactiveSession(), WITHOUT_WORKFLOW_VIEW)).toEqual("activities");
  });

  it("shows the terminal for an active session", () => {
    expect(sessionBaseViewMode(anActiveSession(), WITHOUT_WORKFLOW_VIEW)).toEqual("terminal");
  });

  it("keeps the workflow view for an inactive session that has one", () => {
    expect(sessionBaseViewMode(anInactiveSession(), WITH_WORKFLOW_VIEW)).toEqual("workflow");
  });

  it("keeps the workflow view for an active session that has one", () => {
    expect(sessionBaseViewMode(anActiveSession(), WITH_WORKFLOW_VIEW)).toEqual("workflow");
  });

  it("shows the terminal for a running session awaiting input", () => {
    expect(sessionBaseViewMode(aLiveElicitingSession(), WITHOUT_WORKFLOW_VIEW)).toEqual("terminal");
  });

  it("shows the activities view for a session that died mid-elicitation", () => {
    expect(sessionBaseViewMode(aDeadElicitingSession(), WITHOUT_WORKFLOW_VIEW)).toEqual(
      "activities",
    );
  });

  it("shows the terminal when there is no session", () => {
    expect(sessionBaseViewMode(null, WITHOUT_WORKFLOW_VIEW)).toEqual("terminal");
  });

  it("shows the workflow view when there is no session but a workflow view was resolved", () => {
    // The workflow branch short-circuits ahead of the liveness check, so this pins which of the two
    // wins rather than leaving it to the reading order of the implementation.
    expect(sessionBaseViewMode(null, WITH_WORKFLOW_VIEW)).toEqual("workflow");
  });
});

describe("canResumeSession", () => {
  it("offers resume for an inactive session", () => {
    expect(canResumeSession(anInactiveSession())).toBe(true);
  });

  it("offers resume for an inactive session that shows a workflow view", () => {
    // Resume is keyed on liveness alone, so a dormant pr-stack orchestrator gets the same button
    // in the same place as a dormant terminal session.
    expect(canResumeSession(aSession({ isActive: false, recipe: "pr-stack" }))).toBe(true);
  });

  it("offers resume for a session that died mid-elicitation", () => {
    expect(canResumeSession(aDeadElicitingSession())).toBe(true);
  });

  it("does not offer resume for an active session", () => {
    expect(canResumeSession(anActiveSession())).toBe(false);
  });

  it("does not offer resume for a running session awaiting input", () => {
    expect(canResumeSession(aLiveElicitingSession())).toBe(false);
  });

  it("does not offer resume when there is no session", () => {
    expect(canResumeSession(null)).toBe(false);
  });
});
