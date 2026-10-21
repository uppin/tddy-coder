import { useEffect } from "react";
import type { SessionEntry } from "../../gen/session_pb";
import type { HostConnection } from "../../rpc/connections/types";
import { PARAM_CODE, PARAM_FULL, PARAM_INSPECTOR, type AppLocation } from "../../routing/appLocation";
import type { UseAppLocationResult } from "../../routing/useAppLocation";
import { owningHostForSession } from "../../utils/crossHostSessions";
import { attachActionForSnapshot } from "./attachClaim";
import { nextInspectorState } from "./inspectorState";
import type { InspectorDrawerState } from "./SessionInspectorDrawer";
import type { SessionRuntimeRegistry } from "./sessionRuntimeRegistry";
import type { UseSessionAttachmentResult } from "./useSessionAttachment";
import type { SessionsDrawerAttachClaims } from "./useSessionsDrawerAttachClaims";
import type { useSessionsDrawerInspector } from "./useSessionsDrawerInspector";

export interface UseSessionsDrawerActivationArgs {
  claims: SessionsDrawerAttachClaims;
  location: AppLocation;
  setParams: UseAppLocationResult["setParams"];
  selectedSessionId: string | null;
  inspectorTabParam: string;
  inspectorState: InspectorDrawerState;
  applyInspectorState: ReturnType<typeof useSessionsDrawerInspector>["applyInspectorState"];
  sortedSessions: SessionEntry[];
  selectedSession: SessionEntry | null;
  listGeneration: number;
  runtimeRegistry: SessionRuntimeRegistry;
  attachment: UseSessionAttachmentResult["state"];
  connectSession: UseSessionAttachmentResult["connectSession"];
  restoreAttachment: UseSessionAttachmentResult["restore"];
  resetAttachment: UseSessionAttachmentResult["reset"];
  connectHost: (hostId: string | null) => HostConnection | null;
  selectedInstanceId: string | null;
  sessionToken: string;
}

/**
 * The URL-named session ↔ attachment sync: activate the selected session once the list has loaded,
 * track its liveness across polls, and open the inspector on an attach error. The three effects run
 * in this order, as they always have.
 */
export function useSessionsDrawerActivation({
  claims,
  location,
  setParams,
  selectedSessionId,
  inspectorTabParam,
  inspectorState,
  applyInspectorState,
  sortedSessions,
  selectedSession,
  listGeneration,
  runtimeRegistry,
  attachment,
  connectSession,
  restoreAttachment,
  resetAttachment,
  connectHost,
  selectedInstanceId,
  sessionToken,
}: UseSessionsDrawerActivationArgs): void {
  const {
    attachClaimRef,
    settleAttach,
    activatedSessionIdRef,
    firstActivationPendingRef,
    awaitingCreatedSessionIdRef,
    setUnknownSession,
  } = claims;

  /**
   * Activate whichever session the URL names, once the list has loaded: set the inspector's default
   * state and attach the session. One effect serves every way the selection can change — a drawer
   * click, a deep link on load, Back/Forward, a pasted link — because they all just change the URL.
   *
   * Attaching here rather than in the click handler is what makes Back re-attach: `handleSelect`
   * only navigates.
   */
  useEffect(() => {
    if (!selectedSessionId) {
      activatedSessionIdRef.current = null;
      setUnknownSession(false);
      // `inspector` / `full` / `code` describe a session's panes; with no session selected they
      // describe nothing. Drop them so "back to sessions" cannot leave a URL claiming an open
      // inspector over an empty pane. `replace` — this is cleanup, not a destination.
      if (location.params[PARAM_INSPECTOR] || location.params[PARAM_FULL] || location.params[PARAM_CODE]) {
        setParams(
          { [PARAM_INSPECTOR]: null, [PARAM_FULL]: null, [PARAM_CODE]: null },
          { replace: true },
        );
      }
      return;
    }
    if (activatedSessionIdRef.current === selectedSessionId) return;
    if (sortedSessions.length === 0) return; // list not loaded yet — retry on the next change
    const session = sortedSessions.find((s) => s.sessionId === selectedSessionId);
    if (!session && awaitingCreatedSessionIdRef.current === selectedSessionId) return; // just created — retry on the next change
    awaitingCreatedSessionIdRef.current = null;
    activatedSessionIdRef.current = selectedSessionId;
    const honourUrlInspector = firstActivationPendingRef.current && inspectorTabParam !== "";
    firstActivationPendingRef.current = false;

    if (!session) {
      // The URL names an id that is not in the loaded list — surface a not-found state.
      setUnknownSession(true);
      return;
    }
    setUnknownSession(false);

    // A deep link that asked for a tab keeps it; otherwise the inspector takes its default state,
    // which is closed whatever the session's liveness: an active session shows its terminal, an
    // inactive one shows its recorded activities, and neither has the drawer opened for it.
    if (!honourUrlInspector) {
      const selected = nextInspectorState({ open: false, expanded: false }, { type: "select" });
      applyInspectorState(selected.open ? "open" : "closed", { replace: true });
    }

    // Fast path — the session's runtime is already mounted in the registry (it was attached
    // earlier and stays alive across focus switches). Restore the attachment from the registry's
    // stored connection params so the screen re-evaluates state for the newly selected session
    // WITHOUT an RPC round-trip: no re-connect, no fresh ClaimTerminalControl, no token race, and
    // the existing terminal stream keeps flowing. The registry effect below re-focuses it.
    // The claim is taken inside each branch rather than for any registry hit: a registry entry that
    // is no longer connected restores nothing, so claiming on it would block the resume this dormant
    // session is about to be given.
    const existing = runtimeRegistry.get(selectedSessionId);
    if (existing?.connection) {
      attachClaimRef.current = { sessionId: selectedSessionId, listGeneration };
      restoreAttachment({
        state: { status: "connected", connection: existing.connection },
        hint: existing.hint ?? null,
      });
      return;
    }

    // Slow path — not yet attached. Reset so the attachment effect re-evaluates state for the new
    // selection, then connect to this session's owning daemon. `activeClient` is derived from
    // `selectedSession`, which this render may not have caught up to, so build the client for this
    // session's owner directly rather than reading `activeClient` here.
    resetAttachment();
    // An attach is owed for a live session only; a dormant one has no process to attach to and shows
    // its recorded activities instead. Recording which id this effect took the attach for is what
    // keeps the liveness effect below from issuing a second `ConnectSession` for the same session.
    attachClaimRef.current = session.isActive
      ? { sessionId: selectedSessionId, listGeneration }
      : null;
    if (session.isActive) {
      const owningHost = connectHost(owningHostForSession(session, selectedInstanceId ?? ""));
      if (owningHost) {
        connectSession(selectedSessionId, sessionToken, owningHost)
          .then((attached) => settleAttach(selectedSessionId, attached))
          .catch((err) => {
            console.debug("[SessionsDrawerScreen] connectSession error", err);
          });
      }
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedSessionId, sortedSessions]);

  /**
   * Track the selected session's liveness across polls: attach it when it comes alive, and release
   * the attach claim when it dies.
   *
   * A session can come alive under the operator — resumed from the pane's top bar, or resumed
   * elsewhere and observed on the next list poll — and the activation effect above only runs once per
   * *selection*, so without this the pane would keep showing recorded activities until the session
   * was re-selected. Attaching here is what lets the base view return to the terminal on its own, as
   * `docs/ft/web/inactive-session-activities.md` § Resume states.
   *
   * Releasing on death matters just as much: the same session can be resumed again without the
   * selection ever changing, and that resume owes a fresh `ConnectSession`. Holding the claim past
   * the death would silently swallow every resume after the first.
   *
   * Which of the three the snapshot owes is `attachActionForSnapshot`'s to decide — including why a
   * dormant reading on the claim's own snapshot is not yet evidence of death.
   */
  useEffect(() => {
    if (!selectedSession) return;
    const action = attachActionForSnapshot({
      session: selectedSession,
      claim: attachClaimRef.current,
      listGeneration,
    });
    switch (action) {
      case "hold":
        return;
      case "release":
        attachClaimRef.current = null;
        return;
      case "attach":
        break;
    }
    const owningHost = connectHost(
      owningHostForSession(selectedSession, selectedInstanceId ?? ""),
    );
    if (!owningHost) return;
    attachClaimRef.current = { sessionId: selectedSession.sessionId, listGeneration };
    const attachedId = selectedSession.sessionId;
    connectSession(attachedId, sessionToken, owningHost)
      .then((attached) => settleAttach(attachedId, attached))
      .catch((err) => {
        console.debug("[SessionsDrawerScreen] connectSession error", err);
      });
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedSession, selectedInstanceId, connectHost, listGeneration]);

  // React to attachment status changes: an attach **error** opens the inspector so the operator sees
  // the problem — that is a failure to surface, not a liveness state. Every other status leaves the
  // drawer alone. A session that simply has nothing to attach to ("idle" for an inactive session) no
  // longer opens it: that session's pane shows its recorded activities instead.
  useEffect(() => {
    if (!selectedSessionId) return;
    if (attachment.status === "error" && inspectorState !== "expanded") {
      applyInspectorState("open", { replace: true });
    }
  // These transitions are driven by the attachment, not by the URL: re-running them whenever
  // `inspectorState`/`applyInspectorState` change would fight the operator's own toggles.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [attachment.status, selectedSessionId]);
}
