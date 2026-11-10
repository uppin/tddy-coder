import { useRef, useState } from "react";
import type { AttachClaim } from "./attachClaim";

/**
 * The bookkeeping the sessions drawer's activation, liveness and resume paths share: the attach
 * claim and its retry, the list-snapshot counter, which session was activated, and the not-found
 * state for a deep link that names no known session.
 */
export function useSessionsDrawerAttachClaims() {
  // The attach already taken for the selected session's *current live epoch*. Written by the
  // activation effect (selection), the liveness effect (a selected session that comes alive) and
  // `handleResume`, so no two of them can fire a `ConnectSession` for the same session.
  //
  // "Live epoch" is the load-bearing part: the claim is dropped once a *later* list snapshot reports
  // the session dormant. A session can die and be resumed repeatedly without the selection ever
  // changing, and each revival owes a fresh attach — a claim that only reset on selection change
  // would strand every resume after the first on an empty pane.
  const attachClaimRef = useRef<AttachClaim | null>(null);
  // The session whose last attach failed and was already given its one automatic retry.
  const retriedAttachSessionRef = useRef<string | null>(null);
  /**
   * What a finished `ConnectSession` owes the attach claim.
   *
   * A failed attach took nothing, so its claim is given back and the liveness effect re-attaches on
   * the next list snapshot — a live session offers no Resume to recover by. That is allowed once per
   * session: a second consecutive failure keeps the claim, so a persistent failure stays on the
   * `error` status and inspector instead of spinning a `ConnectSession` per poll.
   */
  const settleAttach = (sessionId: string, attached: boolean) => {
    if (attached) {
      retriedAttachSessionRef.current = null;
      return;
    }
    if (retriedAttachSessionRef.current === sessionId) return;
    retriedAttachSessionRef.current = sessionId;
    if (attachClaimRef.current?.sessionId === sessionId) attachClaimRef.current = null;
  };
  // Bumped once per session-list snapshot, so an attach claim can record which snapshot it was taken
  // under. Without it a resume could not be told apart from a stale dormant reading: `ResumeSession`
  // returns before the daemon's next `ListSessions`, so the list keeps reporting the session dormant
  // for up to one poll after the attach is already established.
  const listGenerationRef = useRef(0);
  // The session id the activation effect below has already run for. Keyed on the id (not a one-shot
  // boolean) so an inbound URL change — Back, a pasted link — activates the newly named session,
  // while a re-render for the same id does not re-connect it.
  const activatedSessionIdRef = useRef<string | null>(null);
  // Whether the activation effect has run at all. The first activation honours an `?inspector=` the
  // URL already carried (a deep link asked for that tab); later ones recompute the default for the
  // newly selected session, as selecting a session in the drawer has always done.
  const firstActivationPendingRef = useRef(true);
  // The session this screen just created and navigated to. `StartSession` returns before the list
  // refresh that carries the new session lands, so until then its absence from an already-loaded
  // list means "not listed yet", not "unknown" — the activation effect waits for it instead.
  const awaitingCreatedSessionIdRef = useRef<string | null>(null);
  // A deep link (`#/sessions/:id`) that resolves to no known session after the list loads shows a
  // not-found state instead of silently no-opping. Driven by local state so it dismisses on Home
  // without depending on a hash change.
  const [unknownSession, setUnknownSession] = useState(false);

  return {
    attachClaimRef,
    settleAttach,
    listGenerationRef,
    activatedSessionIdRef,
    firstActivationPendingRef,
    awaitingCreatedSessionIdRef,
    unknownSession,
    setUnknownSession,
  };
}

export type SessionsDrawerAttachClaims = ReturnType<typeof useSessionsDrawerAttachClaims>;
