import React from "react";
import type { SessionEntry } from "tddy-client-web/gen/session_pb";
import type { SessionAttachmentHint } from "tddy-client-web/connections/session";
import { sessionPaneIsWorkflowView } from "./attachClaim";
import { WorkflowChatScreen } from "tddy-workqueue-web";

/** Extra context a custom workflow view may need beyond the selected session itself. */
export interface WorkflowViewContext {
  /**
   * How the attached session is reached. The chat view derives its own independent connection from
   * this rather than being handed a room from above — see `usePresenterRoom`.
   */
  attachmentHint?: SessionAttachmentHint | null;
}

/**
 * Resolve a custom main-pane view for `session`.
 *
 * Returns `null` when no custom view is registered for the session — callers fall back to the
 * existing terminal / placeholder rendering in that case.
 *
 * Every tddy-coder workflow (`tool`) session gets the single-pane full-screen
 * {@link WorkflowChatScreen} — including a session still recorded with the removed `pr-stack` recipe.
 * Which sessions those are is {@link sessionPaneIsWorkflowView}'s to say: the sessions drawer's attach
 * rules read the same predicate to know a session's pane is not a terminal, and a second copy of the
 * gate here could drift from it — leaving a session the drawer treats as chat rendering a terminal, or
 * the reverse.
 */
export function resolveWorkflowView(
  session: SessionEntry | null,
  context: WorkflowViewContext = {},
): React.ReactNode | null {
  if (!session) return null;
  if (!sessionPaneIsWorkflowView(session)) return null;
  return (
    <WorkflowChatScreen
      key={session.sessionId}
      session={session}
      attachmentHint={context.attachmentHint}
    />
  );
}
