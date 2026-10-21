import { useCallback, useEffect, useMemo, type MutableRefObject } from "react";
import type { SessionEntry } from "../../gen/session_pb";
import { claimAfterFeedEnd, shouldResetAttachmentOnFeedEnd, type AttachClaim } from "./attachClaim";
import type {
  SessionRuntimeConnection,
  SessionRuntimeRegistry,
} from "./sessionRuntimeRegistry";
import type { UseSessionAttachmentResult } from "./useSessionAttachment";

export interface UseSessionsDrawerRuntimeAttachmentArgs {
  attachment: UseSessionAttachmentResult["state"];
  attachmentHint: UseSessionAttachmentResult["hint"];
  resetAttachment: () => void;
  connectedSessionId: string | null;
  runtimeRegistry: SessionRuntimeRegistry;
  runtimes: SessionRuntimeRegistry["runtimes"];
  sortedSessions: SessionEntry[];
  selectedSession: SessionEntry | null;
  attachClaimRef: MutableRefObject<AttachClaim | null>;
}

/**
 * Where the sessions drawer's attachment meets its runtime registry: evicting a runtime whose feed
 * ended, registering a runtime on a successful attach, and the selected session's traffic source.
 */
export function useSessionsDrawerRuntimeAttachment({
  attachment,
  attachmentHint,
  resetAttachment,
  connectedSessionId,
  runtimeRegistry,
  runtimes,
  sortedSessions,
  selectedSession,
  attachClaimRef,
}: UseSessionsDrawerRuntimeAttachmentArgs) {
  // Disconnect a runtime terminal. Evicts the session's runtime; what the claim and the attachment
  // owe that eviction is `claimAfterFeedEnd`'s and `shouldResetAttachmentOnFeedEnd`'s to say, read
  // off the same session list the liveness effect below reads.
  //
  // For a terminal session the feed *is* the attach: it can drop under a session the daemon still
  // reports alive (a `pty_done` on a live agent), so the claim goes with it — holding a claim for an
  // attach that no longer exists is what would strand the pane on the reconnect placeholder, since
  // that effect returns early on the claim and a live session offers no Resume button to recover by
  // hand — and resetting the attachment is how the screen re-evaluates state for the next selection.
  // For a workflow-owned session the ended feed is the hidden runtime layer's, which has no PTY to
  // stream: neither the claim nor the attachment it says anything about belongs to it.
  const onSessionDisconnect = useCallback(
    (sessionId: string) => {
      // Evicting a runtime *releases* its connection, and the attachment names the very same
      // object. So whether the attachment survives is not only a question about the claim: a
      // released connection's `clientFor` and `transport` throw, and an attachment still reporting
      // `{status:"connected", connection}` for one would have every consumer — the inspector's
      // session client, the traffic strip's room, node 4's media gates — reading "connected" off a
      // connection that can no longer answer anything.
      const releasedTheAttachedConnection =
        sessionId === connectedSessionId && runtimeRegistry.get(sessionId)?.connection != null;
      runtimeRegistry.disconnect(sessionId);
      const session = sortedSessions.find((s) => s.sessionId === sessionId);
      if (!session) {
        // The list no longer carries the session, so there is no pane to hold an attach for.
        if (attachClaimRef.current?.sessionId === sessionId) attachClaimRef.current = null;
        if (sessionId === connectedSessionId) resetAttachment();
        return;
      }
      attachClaimRef.current = claimAfterFeedEnd({ claim: attachClaimRef.current, session });
      if (releasedTheAttachedConnection || shouldResetAttachmentOnFeedEnd({ session, connectedSessionId })) {
        resetAttachment();
      }
    },
    [runtimeRegistry, sortedSessions, connectedSessionId, resetAttachment],
  );

  // Register a session in the runtime registry on successful attach, storing the connection
  // params so the runtime layer can render its terminal independently of the focused attachment.
  // `lastDataReceivedAt` starts at the attach moment so the inspector reads "0s ago" before the
  // first DataReceived event lands. Re-attach (an existing backgrounded runtime) refreshes the
  // connection params without resetting byte counters.
  useEffect(() => {
    if (attachment.status !== "connected" || !attachmentHint) return;
    const sessionId = attachment.connection.sessionId;
    const conn: SessionRuntimeConnection = {
      connection: attachment.connection,
      hint: attachmentHint,
    };
    const existing = runtimeRegistry.get(sessionId);
    if (!existing) {
      runtimeRegistry.add(sessionId, {
        sessionId,
        attached: true,
        ...conn,
        bytesIn: 0,
        bytesOut: 0,
        lastDataReceivedAt: Date.now(),
      });
    } else {
      runtimeRegistry.updateConnection(sessionId, conn);
    }
    runtimeRegistry.focus(sessionId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [attachment, attachmentHint]);

  // Compute the inspector traffic source for the selected session: live runtime (active) wins;
  // otherwise the daemon-sourced `SessionEntry` fields (inactive / non-LiveKit sessions).
  const selectedTraffic = useMemo(() => {
    if (!selectedSession) return null;
    const live = runtimeRegistry.get(selectedSession.sessionId);
    if (live) {
      return { bytesIn: live.bytesIn, bytesOut: live.bytesOut, lastDataReceivedAt: live.lastDataReceivedAt };
    }
    const fromEntry = Number(selectedSession.bytesIn ?? 0n) || 0;
    const fromEntryOut = Number(selectedSession.bytesOut ?? 0n) || 0;
    const lastStr = selectedSession.lastDataReceivedAt ?? "";
    const lastNum = lastStr ? Number(lastStr) : null;
    return {
      bytesIn: fromEntry,
      bytesOut: fromEntryOut,
      lastDataReceivedAt: Number.isFinite(lastNum) ? (lastNum as number) : null,
    };
  }, [selectedSession, runtimes, runtimeRegistry]);

  return { onSessionDisconnect, selectedTraffic };
}
