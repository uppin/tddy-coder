import { useCallback, useEffect, type MutableRefObject } from "react";
import type { Client } from "@connectrpc/connect";
import { Signal, type SessionEntry, type SessionService } from "../../gen/session_pb";
import type { HostConnection } from "../../rpc/connections/types";
import { useActionFailure } from "../../hooks/useActionFailure";
import { requestSessionsRefresh } from "../../lib/sessionsRefreshBridge";
import { actionFailed, hostNotConnected } from "../../rpc/rpcFailure";
import { useHostLabel } from "../../rpc/selectedDaemon";
import { owningHostForSession } from "../../utils/crossHostSessions";
import type { AttachClaim } from "./attachClaim";
import type { UseSessionAttachmentResult } from "./useSessionAttachment";

export interface SessionsDrawerPaneActionsOptions {
  selectedSession: SessionEntry | null;
  selectedSessionId: string | null;
  selectedInstanceId: string | null;
  /** The selected session's attachment — its state, and the actions that change it. */
  attachment: Pick<
    UseSessionAttachmentResult,
    "state" | "resumeSession" | "deleteSession" | "signalSession"
  >;
  /** The connection to the selected session's owning host; `null` when no wire reaches it. */
  activeHost: HostConnection | null;
  /** The owning host's `SessionService` client; `null` when no wire reaches it. */
  activeClient: Client<typeof SessionService> | null;
  sessionToken: string;
  attachClaimRef: MutableRefObject<AttachClaim | null>;
  /** The list snapshot the operator is acting on — stamped on the claim a resume takes. */
  listGeneration: number;
}

export interface SessionsDrawerPaneActions {
  /** Why the selected session's last pane action failed, ready to render; `null` while none has. */
  readonly paneError: string | null;
  /** Record an attach that failed — or could not be attempted — as the pane's failure. */
  readonly reportAttachFailure: (error: unknown) => void;
  readonly handleResume: (sessionId: string) => void;
  readonly handleDelete: (sessionId: string) => void;
  readonly handleTerminate: (sessionId: string) => void;
}

/**
 * The selected session's pane actions — Resume, Delete, Terminate — and the one place their
 * failures land.
 *
 * Why the selected session's last pane action failed: an attach, Resume, Delete or Terminate that
 * could not reach the session's owning host, said with the host named. A failure the daemon itself
 * reported lands on the attachment's `error` state instead; both are returned as one `paneError`.
 */
export function useSessionsDrawerPaneActions({
  selectedSession,
  selectedSessionId,
  selectedInstanceId,
  attachment,
  activeHost,
  activeClient,
  sessionToken,
  attachClaimRef,
  listGeneration,
}: SessionsDrawerPaneActionsOptions): SessionsDrawerPaneActions {
  const { state, resumeSession, deleteSession, signalSession } = attachment;
  const hostLabel = useHostLabel();
  const paneFailure = useActionFailure();
  const { report: reportPaneFailure, clear: clearPaneFailure } = paneFailure;

  const owningHostLabel = useCallback(
    () =>
      hostLabel(
        selectedSession ? owningHostForSession(selectedSession, selectedInstanceId ?? "") : (selectedInstanceId ?? ""),
      ),
    [hostLabel, selectedSession, selectedInstanceId],
  );
  const reportAttachFailure = useCallback(
    (err: unknown) => reportPaneFailure(actionFailed("Attach", err)),
    [reportPaneFailure],
  );
  // A pane failure is about the session it happened on, and is settled by that session attaching.
  useEffect(() => clearPaneFailure(), [selectedSessionId, clearPaneFailure]);
  useEffect(() => {
    if (state.status === "connected") clearPaneFailure();
  }, [state.status, clearPaneFailure]);

  const handleResume = (sessionId: string) => {
    clearPaneFailure();
    if (!activeHost) {
      reportPaneFailure(actionFailed("Resume", hostNotConnected(owningHostLabel())));
      return;
    }
    // `ResumeSession` returns the session's routing and `useSessionAttachment` puts the screen
    // straight into `connected` — the attach for this live epoch is already done.
    // Claiming it here stops the liveness effect from firing a second `ConnectSession` when the next
    // list poll reports the session alive, which would mint a fresh browser identity and bounce the
    // terminal through a reconnect and another `ClaimTerminalControl`. The claim is stamped with the
    // snapshot the operator acted on, so the dormant readings still in flight cannot revoke it.
    attachClaimRef.current = { sessionId, listGeneration };
    // `resumeSession` never rejects: a refused resume is the attachment's `error` state, shown below.
    void resumeSession(sessionId, sessionToken, activeHost).then((attached) => {
      // The resume never landed, so no attach was taken after all — release the claim so the
      // liveness effect can still attach if the session turns out to be alive.
      if (!attached) attachClaimRef.current = null;
    });
  };

  const handleDelete = (sessionId: string) => {
    clearPaneFailure();
    if (!activeClient) {
      reportPaneFailure(actionFailed("Delete", hostNotConnected(owningHostLabel())));
      return;
    }
    // Never rejects — a refused delete is the attachment's `error` state.
    void deleteSession(sessionId, sessionToken, activeClient);
  };

  const handleTerminate = (sessionId: string) => {
    clearPaneFailure();
    if (!activeClient) {
      reportPaneFailure(actionFailed("Terminate", hostNotConnected(owningHostLabel())));
      return;
    }
    // Never rejects — a refused signal is the attachment's `error` state. Common cause: the session
    // already ended (e.g. process exited on its own) before this click reached the daemon — the
    // refresh below still corrects the stale `isActive` that showed "Terminate" for a dead session.
    void signalSession(sessionId, Signal.SIGTERM, sessionToken, activeClient)
      .finally(() => {
        // The daemon computes `isActive` from live PID liveness, not from a push update — refetch
        // so the row (and its "Terminate" button) reflects the session's actual current state.
        requestSessionsRefresh();
      });
  };

  const paneError = paneFailure.error ?? (state.status === "error" ? state.error : null);

  return { paneError, reportAttachFailure, handleResume, handleDelete, handleTerminate };
}
