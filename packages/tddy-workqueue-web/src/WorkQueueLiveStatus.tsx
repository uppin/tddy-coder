import React from "react";
import type { WorkQueueLiveness } from "./useWorkQueueWatch";

/** The reason a watch is not delivering, or `null` while it is (or has yet to). */
function trouble(liveness: WorkQueueLiveness): { reason: string; retrying: boolean } | null {
  if (liveness.state === "reconnecting") return { reason: liveness.reason, retrying: true };
  if (liveness.state === "ended") return { reason: liveness.reason, retrying: false };
  return null;
}

/** Whether the queue could not be loaded at all: no copy of it arrived, and the watch says why. */
export function failedToLoad(loaded: boolean, liveness: WorkQueueLiveness): boolean {
  return !loaded && trouble(liveness) !== null;
}

/** Shown in place of the list when no copy of the queue arrived — an unloaded queue is not an empty one. */
export function WorkQueueLoadError({ liveness }: { liveness: WorkQueueLiveness }): React.ReactElement | null {
  const problem = trouble(liveness);
  if (!problem) return null;
  return (
    <div data-testid="work-queue-load-error" role="alert" className="m-3 rounded border border-destructive/40 p-3 text-sm text-destructive">
      Could not load this work queue: {problem.reason}
      {problem.retrying && <span className="text-muted-foreground"> — retrying…</span>}
    </div>
  );
}

/** Above a loaded queue that is no longer kept current: reconnecting, or stopped and why. */
export function WorkQueueLiveNotice({ liveness }: { liveness: WorkQueueLiveness }): React.ReactElement | null {
  const problem = trouble(liveness);
  if (!problem) return null;
  return (
    <p data-testid="work-queue-live-notice" role="status" className="flex-shrink-0 px-3 pt-2 text-xs text-muted-foreground">
      {problem.retrying
        ? `Live updates interrupted (${problem.reason}). Reconnecting…`
        : `Live updates stopped: ${problem.reason}`}
    </p>
  );
}
