import React from "react";
import type { Client } from "@connectrpc/connect";
import type { ConnectionService, SessionEntry } from "../../gen/connection_pb";
import type { SessionAttachmentHint } from "../../rpc/connections/session";
import type { SessionMetadata } from "../../lib/sessionParticipantMetadata";
import { sessionPaneIsWorkflowView } from "./attachClaim";
import { PrStackScreen } from "./prstack/PrStackScreen";
import { WorkflowChatScreen } from "./WorkflowChatScreen";

type ConnectionClient = Client<typeof ConnectionService>;

/** Extra context a custom workflow view may need beyond the selected session itself. */
export interface WorkflowViewContext {
  client?: ConnectionClient;
  sessionToken?: string;
  /**
   * How the attached session is reached. Custom views that need a LiveKit room (e.g. the PR-Stack
   * Chat Screen) derive their own independent connection from this rather than being handed a
   * room from above — see `usePresenterLiveKitRoom`.
   */
  attachmentHint?: SessionAttachmentHint | null;
  /** The full session list — the PR-Stack view resolves each node's in-progress child by branch. */
  sessions?: SessionEntry[];
  /**
   * The session's project default branch (`ProjectEntry.main_branch_ref`). The PR-Stack view names it
   * as a root node's spawn base and as the branch a repoint would land a stranded node on; it is empty
   * for a legacy project that stores none (D20).
   */
  defaultBranch?: string;
  /**
   * The session's project resolved default remote (`ProjectEntry.default_remote`, e.g. `origin`,
   * `upstream`). The PR-Stack view prepends it to the local branch names a planned-PR child session's
   * "Base branch" picker offers, so the value sent as `selected_integration_base_ref` is the
   * `<remote>/<branch>` ref the daemon fetches — not a bare local name whose first path segment it
   * would mistake for a remote. Empty for a legacy project that stored none; the view falls back to
   * `origin` (the daemon's own last resort).
   */
  defaultRemote?: string;
  /** Fired after a child session is spawned inside the view — see `PrStackScreenProps.onChildSessionStarted`. */
  onChildSessionStarted?: (entry: {
    sessionId: string;
    recipe: string;
    orchestratorSessionId: string;
    projectId: string;
  }) => void;
  /**
   * Select and attach an existing session. The PR-Stack view opens a spawned planned PR's bound child
   * session with it — see `PrStackScreenProps.onOpenSession`.
   */
  onOpenSession?: (sessionId: string) => void;
  /**
   * The `session` metadata block each live participant publishes, keyed by session id. The PR-Stack
   * view joins its planned nodes to child sessions with it — the only signal that crosses a host
   * boundary, and the only carrier of `stack_node_id` (D37, D38).
   */
  sessionMetadataBySessionId?: ReadonlyMap<string, SessionMetadata>;
}

/**
 * Resolve a custom main-pane view for `session`, keyed by `session.recipe`.
 *
 * Returns `null` when no custom view is registered for the session — callers fall back to the
 * existing terminal / placeholder rendering in that case.
 *
 * `pr-stack` gets its own two-pane screen (planned-PR list + chat). Every other tddy-coder workflow
 * (`tool`) session gets the single-pane full-screen {@link WorkflowChatScreen}. Which sessions those
 * are is {@link sessionPaneIsWorkflowView}'s to say: the sessions drawer's attach rules read the same
 * predicate to know a session's pane is not a terminal, and a second copy of the gate here could
 * drift from it — leaving a session the drawer treats as chat rendering a terminal, or the reverse.
 */
export function resolveWorkflowView(
  session: SessionEntry | null,
  context: WorkflowViewContext = {},
): React.ReactNode | null {
  if (!session) return null;
  if (!sessionPaneIsWorkflowView(session)) return null;
  if (session.recipe === "pr-stack") {
    return (
      <PrStackScreen
        key={session.sessionId}
        session={session}
        client={context.client}
        sessionToken={context.sessionToken}
        sessions={context.sessions}
        attachmentHint={context.attachmentHint}
        defaultBranch={context.defaultBranch}
        defaultRemote={context.defaultRemote}
        onChildSessionStarted={context.onChildSessionStarted}
        onOpenSession={context.onOpenSession}
        sessionMetadataBySessionId={context.sessionMetadataBySessionId}
      />
    );
  }
  // Past the gate and not `pr-stack`: a tddy-coder workflow session, which the chat screen owns.
  return (
    <WorkflowChatScreen
      key={session.sessionId}
      session={session}
      attachmentHint={context.attachmentHint}
    />
  );
}
