import { describe, expect, it } from "bun:test";
import {
  attachActionForSnapshot,
  claimAfterFeedEnd,
  sessionPaneIsWorkflowView,
  shouldResetAttachmentOnFeedEnd,
  type AttachCandidate,
  type AttachClaim,
} from "./attachClaim";

/**
 * The pure attach-claim state machine behind the sessions drawer's re-attach loop
 * (`SessionsDrawerScreen`).
 *
 * The rule worth pinning here is that a terminal feed ending must not release the claim of a
 * session whose main pane is a workflow view. A `pr-stack` session renders `PrStackScreen` over a
 * mounted-but-hidden runtime layer (`SessionMainPane`), so a LiveKit terminal attaches to a session
 * whose real surface is ACP chat and reports "remote session ended" almost immediately. Releasing
 * the claim on that signal hands the re-attach straight back to the liveness effect, which attaches
 * again on the next 2s list poll — a `ConnectSession` per poll, forever, for as long as the session
 * stays alive. That is the loop observed against session 01a051f0 on host `udoo`.
 */

const PR_STACK_SESSION = "01a051f0-482d-7841-927d-a8983d2af9a8";
const TERMINAL_SESSION = "019fc148-ae2c-7363-98a1-269c46f432ce";

function aSession(overrides: Partial<AttachCandidate> = {}): AttachCandidate {
  return {
    sessionId: TERMINAL_SESSION,
    isActive: true,
    sessionType: "claude-cli",
    recipe: "",
    ...overrides,
  };
}

/** A live `pr-stack` orchestrator: chat surface, no PTY of its own. */
function aPrStackSession(overrides: Partial<AttachCandidate> = {}): AttachCandidate {
  return aSession({
    sessionId: PR_STACK_SESSION,
    sessionType: "tool",
    recipe: "pr-stack",
    ...overrides,
  });
}

function aClaimFor(sessionId: string, listGeneration: number): AttachClaim {
  return { sessionId, listGeneration };
}

describe("sessionPaneIsWorkflowView", () => {
  it("reports a pr-stack session as workflow-owned", () => {
    // Given a live pr-stack orchestrator
    const session = aPrStackSession();

    // When the pane owner is resolved
    const workflowOwned = sessionPaneIsWorkflowView(session);

    // Then
    expect(workflowOwned).toBe(true);
  });

  it("reports a pr-stack session as workflow-owned whatever its session type", () => {
    // Given a pr-stack recipe recorded against a claude-cli session: PrStackScreen is keyed on the
    // recipe alone, so the pane is still the two-pane screen and not a terminal.
    const session = aSession({ sessionType: "claude-cli", recipe: "pr-stack" });

    // When the pane owner is resolved
    const workflowOwned = sessionPaneIsWorkflowView(session);

    // Then
    expect(workflowOwned).toBe(true);
  });

  it("reports a tool session carrying a recipe as workflow-owned", () => {
    // Given a tddy-coder tool session running a non-pr-stack recipe
    const session = aSession({ sessionType: "tool", recipe: "grill-me" });

    // When the pane owner is resolved
    const workflowOwned = sessionPaneIsWorkflowView(session);

    // Then
    expect(workflowOwned).toBe(true);
  });

  it("reports a claude-cli session carrying a recipe as terminal-owned", () => {
    // Given a PTY session that carries a managed recipe but has no Presenter surface
    const session = aSession({ sessionType: "claude-cli", recipe: "grill-me" });

    // When the pane owner is resolved
    const workflowOwned = sessionPaneIsWorkflowView(session);

    // Then
    expect(workflowOwned).toBe(false);
  });

  it("reports a tool session with no recipe as terminal-owned", () => {
    // Given a bare tool session
    const session = aSession({ sessionType: "tool", recipe: "" });

    // When the pane owner is resolved
    const workflowOwned = sessionPaneIsWorkflowView(session);

    // Then
    expect(workflowOwned).toBe(false);
  });
});

describe("claimAfterFeedEnd", () => {
  it("keeps the attach claim when a terminal feed ends under a workflow-owned session", () => {
    // Given a pr-stack session attached under snapshot 7
    const session = aPrStackSession();
    const claim = aClaimFor(PR_STACK_SESSION, 7);

    // When its hidden terminal reports the remote session ended
    const next = claimAfterFeedEnd({ claim, session });

    // Then the claim survives, so the next list poll does not re-attach
    expect(next).toEqual(aClaimFor(PR_STACK_SESSION, 7));
  });

  it("releases the attach claim when a terminal feed ends under a terminal-owned session", () => {
    // Given a claude-cli session attached under snapshot 7
    const session = aSession();
    const claim = aClaimFor(TERMINAL_SESSION, 7);

    // When its terminal — the session's actual surface — reports the remote session ended
    const next = claimAfterFeedEnd({ claim, session });

    // Then the claim is released so the liveness effect can re-attach
    expect(next).toBeNull();
  });

  it("leaves a claim held for a different session untouched", () => {
    // Given a claim held for the pr-stack session
    const claim = aClaimFor(PR_STACK_SESSION, 7);

    // When an unrelated terminal session's feed ends
    const next = claimAfterFeedEnd({ claim, session: aSession() });

    // Then
    expect(next).toEqual(aClaimFor(PR_STACK_SESSION, 7));
  });

  it("stays released when no claim is held", () => {
    // Given no attach claim
    // When a terminal feed ends
    const next = claimAfterFeedEnd({ claim: null, session: aSession() });

    // Then
    expect(next).toBeNull();
  });
});

describe("attachActionForSnapshot", () => {
  it("attaches a live session that holds no claim", () => {
    // Given a live session nothing has attached yet
    const session = aSession();

    // When snapshot 3 arrives
    const action = attachActionForSnapshot({ session, claim: null, listGeneration: 3 });

    // Then
    expect(action).toBe("attach");
  });

  it("holds a live session the claim already covers", () => {
    // Given a live session attached under snapshot 3
    const session = aSession();
    const claim = aClaimFor(TERMINAL_SESSION, 3);

    // When a later snapshot arrives
    const action = attachActionForSnapshot({ session, claim, listGeneration: 4 });

    // Then no second ConnectSession is issued for the same attach
    expect(action).toBe("hold");
  });

  it("attaches a live session whose claim belongs to another session", () => {
    // Given the claim is held for the pr-stack session while a terminal session is selected
    const claim = aClaimFor(PR_STACK_SESSION, 3);

    // When snapshot 4 arrives for the terminal session
    const action = attachActionForSnapshot({ session: aSession(), claim, listGeneration: 4 });

    // Then
    expect(action).toBe("attach");
  });

  it("releases the claim on a later snapshot reporting the session dormant", () => {
    // Given a session attached under snapshot 3 that has since died
    const session = aSession({ isActive: false });
    const claim = aClaimFor(TERMINAL_SESSION, 3);

    // When snapshot 4 reports it dormant
    const action = attachActionForSnapshot({ session, claim, listGeneration: 4 });

    // Then
    expect(action).toBe("release");
  });

  it("holds the claim on the snapshot it was taken under when the session reads dormant", () => {
    // Given a resume issued under snapshot 3: ResumeSession returns before the daemon's next
    // ListSessions, so that same snapshot still reports the session dormant.
    const session = aSession({ isActive: false });
    const claim = aClaimFor(TERMINAL_SESSION, 3);

    // When the action is resolved for that very snapshot
    const action = attachActionForSnapshot({ session, claim, listGeneration: 3 });

    // Then the claim is kept, so the resume is not handed a duplicate ConnectSession
    expect(action).toBe("hold");
  });

  it("holds a dormant session that holds no claim", () => {
    // Given a dormant session with nothing attached
    const session = aSession({ isActive: false });

    // When snapshot 4 arrives
    const action = attachActionForSnapshot({ session, claim: null, listGeneration: 4 });

    // Then a dormant session is never attached — it shows its recorded activities instead
    expect(action).toBe("hold");
  });

  it("holds a live workflow session whose claim survived its terminal feed ending", () => {
    // Given a pr-stack session still holding the claim its hidden terminal could not release
    const session = aPrStackSession();
    const claim = claimAfterFeedEnd({ claim: aClaimFor(PR_STACK_SESSION, 7), session });

    // When the next list poll arrives
    const action = attachActionForSnapshot({ session, claim, listGeneration: 8 });

    // Then the poll issues no ConnectSession — the re-attach loop is closed
    expect(action).toBe("hold");
  });
});

describe("shouldResetAttachmentOnFeedEnd", () => {
  it("resets the attachment when a terminal-owned connected session's feed ends", () => {
    // Given the connected session is the claude-cli one whose terminal just ended
    const session = aSession();

    // When the reset is decided
    const reset = shouldResetAttachmentOnFeedEnd({ session, connectedSessionId: TERMINAL_SESSION });

    // Then the screen re-evaluates state for the next selection, as it always has
    expect(reset).toBe(true);
  });

  it("keeps the attachment when a workflow-owned connected session's feed ends", () => {
    // Given the connected session is the pr-stack one, whose hidden terminal ended while its chat
    // surface derives its LiveKit room from that very attachment
    const session = aPrStackSession();

    // When the reset is decided
    const reset = shouldResetAttachmentOnFeedEnd({ session, connectedSessionId: PR_STACK_SESSION });

    // Then the chat surface keeps the room it is talking over
    expect(reset).toBe(false);
  });

  it("keeps the attachment when the ended feed is not the connected session's", () => {
    // Given a backgrounded terminal session's feed ends while the pr-stack session holds the attachment
    const session = aSession();

    // When the reset is decided
    const reset = shouldResetAttachmentOnFeedEnd({ session, connectedSessionId: PR_STACK_SESSION });

    // Then
    expect(reset).toBe(false);
  });

  it("keeps the attachment when no session is connected", () => {
    // Given nothing is attached
    const session = aSession();

    // When the reset is decided
    const reset = shouldResetAttachmentOnFeedEnd({ session, connectedSessionId: null });

    // Then
    expect(reset).toBe(false);
  });
});
