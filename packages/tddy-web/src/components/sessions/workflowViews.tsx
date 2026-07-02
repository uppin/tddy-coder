import React from "react";
import type { Client } from "@connectrpc/connect";
import type { ConnectionService, SessionEntry } from "../../gen/connection_pb";
import type { SessionAttachmentState } from "./useSessionAttachment";
import { PrStackScreen } from "./prstack/PrStackScreen";

type ConnectionClient = Client<typeof ConnectionService>;

/** Extra context a custom workflow view may need beyond the selected session itself. */
export interface WorkflowViewContext {
  client?: ConnectionClient;
  sessionToken?: string;
  /**
   * The session's own attach state. Custom views that need a LiveKit room (e.g. the PR-Stack
   * Chat Screen) derive their own independent connection from this rather than being handed a
   * room from above — see `usePresenterLiveKitRoom`.
   */
  attachment?: SessionAttachmentState;
  /** Fired after a child session is spawned inside the view — see `PrStackScreenProps.onChildSessionStarted`. */
  onChildSessionStarted?: (entry: {
    sessionId: string;
    recipe: string;
    orchestratorSessionId: string;
    projectId: string;
  }) => void;
}

/**
 * Resolve a custom main-pane view for `session`, keyed by `session.recipe`.
 *
 * Returns `null` when no custom view is registered for the recipe — callers fall back to the
 * existing terminal / placeholder rendering in that case. This registry is intentionally tiny;
 * it is expected to grow one `if` per future per-workflow screen.
 */
export function resolveWorkflowView(
  session: SessionEntry | null,
  context: WorkflowViewContext = {},
): React.ReactNode | null {
  if (!session) return null;
  if (session.recipe === "pr-stack") {
    return (
      <PrStackScreen
        key={session.sessionId}
        session={session}
        client={context.client}
        sessionToken={context.sessionToken}
        attachment={context.attachment}
        onChildSessionStarted={context.onChildSessionStarted}
      />
    );
  }
  return null;
}
