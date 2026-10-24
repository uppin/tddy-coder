import type { CloneProgress, StartPhase_Step, StartSessionEvent } from "../../gen/session_pb";

/** One step of a start as the create pane lists it. */
export interface StartProgressStep {
  step: StartPhase_Step;
  state: "active" | "done" | "failed";
}

/**
 * What a streamed start has reported so far: its steps in the order the host began them, the latest
 * clone progress while the clone step runs, and whether the stream failed (the active step then
 * becomes `failed`).
 */
export interface StartProgressState {
  steps: StartProgressStep[];
  clone: CloneProgress | null;
}

export const NO_START_PROGRESS: StartProgressState = { steps: [], clone: null };

/** Fold one `StartSessionEvent` into the state. Attachment progress and keep-alives change nothing. */
export function startProgressAfter(
  state: StartProgressState,
  event: StartSessionEvent,
): StartProgressState {
  // TODO(progress-ui): implement — phase BEGIN appends active, END marks done, clone_progress replaces clone
  void event;
  return state;
}

/** The state after the stream failed: the step in progress is marked failed. */
export function startProgressFailed(state: StartProgressState): StartProgressState {
  // TODO(progress-ui): implement
  return state;
}
