import React from "react";
import { InlineError } from "../ui/inline-error";
import type { BulkDeleteFailure } from "./useSessionsDrawerDeleteSelection";

/**
 * Screen-wide reads that failed: the registry unscoped sessions resolve their project from, and the
 * feed the row dots follow. Quiet, one line each, gone once the read recovers.
 */
export function SessionsDrawerLoadNotices({
  projectsError,
  notificationsError,
}: {
  projectsError: string | null;
  notificationsError: string | null;
}) {
  if (!projectsError && !notificationsError) return null;
  return (
    <div className="flex-shrink-0 px-3 py-1 border-b border-border space-y-0.5">
      <InlineError
        testId="sessions-drawer-projects-load-error"
        message={projectsError && `Projects unavailable: ${projectsError}`}
        className="text-xs"
      />
      <InlineError
        testId="sessions-drawer-notifications-error"
        message={notificationsError && `Session notifications unavailable: ${notificationsError}`}
        className="text-xs"
      />
    </div>
  );
}

/**
 * The drawer list's own notices: why the list could not be loaded, and one line per row the last
 * bulk delete left behind. `undefined` when there is nothing to say, so the drawer draws no slot.
 */
export function sessionsDrawerListNotices(
  listLoadError: string | null,
  bulkDeleteFailures: readonly BulkDeleteFailure[],
): React.ReactNode | undefined {
  if (!listLoadError && bulkDeleteFailures.length === 0) return undefined;
  return (
    <>
      <InlineError testId="sessions-drawer-load-error" message={listLoadError} className="text-xs" />
      {bulkDeleteFailures.map((failure) => (
        <InlineError
          key={failure.sessionId}
          testId={`sessions-drawer-bulk-delete-error-${failure.sessionId}`}
          message={`Could not delete ${failure.label}: ${failure.reason}`}
          className="text-xs"
        />
      ))}
    </>
  );
}

/**
 * The selected session's last failed action, over its pane: what the operator just tried on this
 * session, and why it did not work.
 */
export function SessionsDrawerPaneError({ message }: { message: string | null }) {
  if (!message) return null;
  return (
    <InlineError
      testId="sessions-pane-error"
      message={message}
      className="absolute bottom-3 left-1/2 z-40 max-w-[min(90%,40rem)] -translate-x-1/2 rounded-md border border-destructive/40 bg-background px-3 py-1.5 shadow-md"
    />
  );
}
