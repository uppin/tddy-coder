/**
 * The pure attach-claim rules behind the sessions drawer's attach/re-attach decisions.
 *
 * They live outside `SessionsDrawerScreen` because they are the whole of what stops a live session
 * being re-attached once per list poll: a `ConnectSession` every 2s, each one joining and leaving
 * LiveKit under a fresh identity. Keeping them here makes the loop's closing conditions testable
 * without a rendered screen, and keeps the pane-ownership question answered in exactly one place.
 */

/**
 * An attach already taken for one session, stamped with the session-list snapshot it was taken
 * under. The stamp is what lets a dormant reading be judged as evidence or as staleness: a list
 * snapshot the claim predates has genuinely observed the session die, while the snapshot the claim
 * was made under simply has not caught up with it yet.
 */
export interface AttachClaim {
  readonly sessionId: string;
  readonly listGeneration: number;
}

/** The fields of a `SessionEntry` the attach rules read. */
export interface AttachCandidate {
  readonly sessionId: string;
  readonly isActive: boolean;
  readonly sessionType: string;
  readonly recipe: string;
}

/**
 * What a session-list snapshot owes the selected session: take the attach (`attach`), give the claim
 * back so a later revival can be attached (`release`), or leave the claim as it stands (`hold`).
 */
export type AttachAction = "attach" | "hold" | "release";

/**
 * Whether the session's main pane is owned by a workflow view rather than by a terminal.
 *
 * `SessionMainPane` renders such a view *over* a still-mounted, CSS-hidden runtime layer — unmounting
 * it would cancel the session's agent conversations on the daemon — so the hidden terminal attaches
 * to a session whose real surface is ACP chat, finds no PTY to stream, and reports the remote session
 * ended. Everything about the pane is fine; only the terminal's own feed is over. The attach rules
 * need to tell the two apart, and `resolveWorkflowView` selects the view off the same predicate, so
 * the two readings of "is this a terminal session" cannot drift apart.
 */
export function sessionPaneIsWorkflowView(
  session: Pick<AttachCandidate, "sessionType" | "recipe">,
): boolean {
  if (session.recipe === "pr-stack") return true;
  const isToolSession = session.sessionType === "" || session.sessionType === "tool";
  return isToolSession && session.recipe !== "";
}

/**
 * The claim to hold after a terminal feed ends under `session`.
 *
 * For a terminal-owned session the feed *is* the attach, so the claim dies with it and the liveness
 * effect is free to re-attach on the next snapshot if the session is still alive. For a
 * workflow-owned session the feed is a hidden layer's, and its ending says nothing about the attach
 * the chat surface is using — releasing the claim there is what hands the re-attach straight back to
 * the liveness effect and spins the ConnectSession-per-poll loop.
 */
export function claimAfterFeedEnd({
  claim,
  session,
}: {
  claim: AttachClaim | null;
  session: AttachCandidate;
}): AttachClaim | null {
  if (claim?.sessionId !== session.sessionId) return claim;
  return sessionPaneIsWorkflowView(session) ? claim : null;
}

/**
 * The action a session-list snapshot owes `session`.
 *
 * A dormant session has no process to attach to and shows its recorded activities instead, so it is
 * only ever held or released. Releasing waits for a snapshot the claim predates: `ResumeSession`
 * returns before the daemon's next `ListSessions`, so the snapshot a resume was made under still
 * reports the session dormant, and releasing on that reading would hand the resume a duplicate
 * `ConnectSession`. A live session is attached exactly once per claim — the claim is what keeps a
 * second `ConnectSession` from being issued for an attach that already exists.
 */
export function attachActionForSnapshot({
  session,
  claim,
  listGeneration,
}: {
  session: AttachCandidate;
  claim: AttachClaim | null;
  listGeneration: number;
}): AttachAction {
  const claimsThisSession = claim?.sessionId === session.sessionId;
  if (!session.isActive) {
    if (claimsThisSession && listGeneration > claim.listGeneration) return "release";
    return "hold";
  }
  return claimsThisSession ? "hold" : "attach";
}
