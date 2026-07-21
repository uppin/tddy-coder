import React from "react";
import type { Client } from "@connectrpc/connect";
import type { ConnectionService, SessionEntry } from "../../gen/connection_pb";
import type { SessionAttachmentState } from "./useSessionAttachment";
import { PrStackScreen } from "./prstack/PrStackScreen";
import { WorkflowChatScreen } from "./WorkflowChatScreen";

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
 * Returns `null` when no custom view is registered for the session — callers fall back to the
 * existing terminal / placeholder rendering in that case.
 *
 * `pr-stack` gets its own two-pane screen (planned-PR list + chat). Every other tddy-coder workflow
 * (`tool`) session gets the single-pane full-screen {@link WorkflowChatScreen}. The gate is
 * `session_type` ∈ {"", "tool"} plus a non-empty `recipe`: only tddy-coder `tool` sessions run a
 * Presenter/ACP surface the chat can reach, so `claude-cli` / `cursor-cli` PTY sessions (which have
 * no Presenter, even when they carry a managed `recipe`) fall through to the terminal.
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
  const isToolSession = session.sessionType === "" || session.sessionType === "tool";
  if (isToolSession && session.recipe !== "") {
    return (
      <WorkflowChatScreen
        key={session.sessionId}
        session={session}
        attachment={context.attachment}
      />
    );
  }
  return null;
}
