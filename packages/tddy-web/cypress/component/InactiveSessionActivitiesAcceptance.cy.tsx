/**
 * Acceptance: an **inactive session shows its recorded activities**, not the inspector.
 *
 * Selecting a dormant session used to open the Inspector over an empty pane whose only content was
 * "Select Resume to reconnect". It now shows the session's recorded ACP transcript as the main-pane
 * view, keeps the Inspector closed, and offers Resume from the pane's top bar — in the same position
 * for every inactive session, including the ones whose base view (PR-Stack, workflow chat) is
 * deliberately left alone.
 *
 * PRD: docs/ft/web/inactive-session-activities.md
 *
 * Two mounting levels, mirroring `SessionInactiveInspectorOverlay`:
 * - full `SessionsDrawerScreen` for selection, inspector defaults, Resume routing, and URL state;
 * - `SessionMainPane` direct for base-view precedence and for the one case that needs an explicit
 *   mounted runtime behind the view.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { createClient, type Transport } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  ConnectionService,
  ClaimTerminalControlResponseSchema,
  TerminalControlEventSchema,
} from "../../src/gen/connection_pb";
import type { SessionEntry } from "../../src/gen/connection_pb";
import type { SessionRuntimeState } from "../../src/components/sessions/sessionRuntimeRegistry";
import type { SessionAttachmentState } from "../../src/components/sessions/useSessionAttachment";
import { aSessionConnection } from "../support/rpc/sessionConnections";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { SessionMainPane } from "../../src/components/sessions/SessionMainPane";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  acpReplayHandlers,
  aHeldCountReplay,
  aToolDetail,
  replayAgentText,
  replayToolCall,
  ToolCallStatus,
  ToolKind,
} from "../support/rpc/acpReplay";
import { sessionsDrawerPage as page } from "../support/pages/sessionsDrawerPage";
import { sessionActivitiesPage } from "../support/pages/sessionActivitiesPage";
import { agentActivityPage } from "../support/pages/agentActivityPage";
import { agentChatPage } from "../support/pages/agentChatPage";
import { appLocationPage } from "../support/pages/appLocationPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { workflowChatScreenPage } from "../support/pages/workflowChatScreenPage";

// ---------------------------------------------------------------------------
// Session fixtures
// ---------------------------------------------------------------------------

const DORMANT = {
  sessionId: "dormant-aaaaaaaa-0000-0000-0000-000000000000",
  createdAt: "2026-08-01T09:00:00Z",
  status: "exited",
  repoPath: "/home/dev/finished-work",
  pid: 0,
  isActive: false,
  projectId: "proj-activities-1",
  daemonInstanceId: "",
  workflowGoal: "Parser rewrite",
  pendingElicitation: false,
};

const LIVE = {
  ...DORMANT,
  sessionId: "live-bbbbbbbb-0000-0000-0000-000000000000",
  status: "active",
  pid: 31000,
  isActive: true,
  workflowGoal: "Ongoing work",
};

const DORMANT_PR_STACK = {
  ...DORMANT,
  sessionId: "dormant-prstack-cccccccc-0000-0000-0000-000000000000",
  recipe: "pr-stack",
  sessionType: "tool",
  workflowGoal: "Stack the parser PRs",
};

const DORMANT_WORKFLOW = {
  ...DORMANT,
  sessionId: "dormant-workflow-dddddddd-0000-0000-0000-000000000000",
  recipe: "plan-tdd-one-shot",
  sessionType: "tool",
  workflowGoal: "Plan the migration",
};

/** The transcript the daemon replays for a dormant session: one agent line, one completed tool call. */
const RECORDED_TRANSCRIPT = {
  counts: [2],
  snapshot: [
    replayAgentText("Rewrote the tokenizer.", 1_000),
    replayToolCall({
      id: "tool-1",
      title: "Edit tokenizer.rs",
      kind: ToolKind.EDIT,
      status: ToolCallStatus.COMPLETED,
      atUnixMs: 3_000,
    }),
  ],
  details: { "tool-1": aToolDetail({ input: { path: "tokenizer.rs" }, output: { applied: true } }) },
};

/** A session that recorded nothing at all — the empty-transcript case. */
const NO_TRANSCRIPT = { counts: [0], snapshot: [] };

// ---------------------------------------------------------------------------
// Mount helpers
// ---------------------------------------------------------------------------

function mountScreen(backend: ReturnType<typeof aConnectionServiceBackend>) {
  mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
}

const noopHandlers = {
  onToggleInspector: () => undefined,
  onInspectorClose: () => undefined,
  onInspectorExpand: () => undefined,
  onInspectorRestore: () => undefined,
  onResume: () => undefined,
  onDelete: () => undefined,
  onTerminate: () => undefined,
};

/** A client serving the recorded transcript and nothing else — enough for the Activities view. */
function aReplayClient(scenario = RECORDED_TRANSCRIPT) {
  const backend = anInMemoryRpcBackend().implement(
    ConnectionService,
    acpReplayHandlers(scenario),
  );
  return createClient(ConnectionService, backend.transport());
}

const OTHER_SCREEN = "screen-held-by-another-9999";

/** A client serving the transcript AND a terminal whose control lease is held elsewhere, so a
 *  mounted runtime would show its "Claim terminal" CTA if it were rendered in the foreground. */
function aReplayTransportWithHeldTerminal() {
  const backend = anInMemoryRpcBackend().implement(ConnectionService, {
    ...acpReplayHandlers(RECORDED_TRANSCRIPT),
    claimTerminalControl: async () =>
      create(ClaimTerminalControlResponseSchema, {
        granted: false,
        currentHolderScreenId: OTHER_SCREEN,
      }),
    watchTerminalControl: async function* (_req: unknown, context: { signal: AbortSignal }) {
      yield create(TerminalControlEventSchema, {
        holderScreenId: OTHER_SCREEN,
        youAreController: false,
      });
      await new Promise<void>((resolve) =>
        context.signal.addEventListener("abort", () => resolve(), { once: true }),
      );
    },
    streamTerminalOutput: async function* (_req: unknown, context: { signal: AbortSignal }) {
      await new Promise<void>((resolve) =>
        context.signal.addEventListener("abort", () => resolve(), { once: true }),
      );
    },
  });
  return backend.transport();
}

/** A runtime whose session is served by its host itself — plain RPC over `transport`. */
const aHostServedRuntimeFor = (sessionId: string, transport: Transport): SessionRuntimeState => ({
  sessionId,
  connection: aSessionConnection(sessionId).servingOver(transport).build(),
  hint: { sessionId },
  attached: true,
  bytesIn: 0,
  bytesOut: 0,
  lastDataReceivedAt: null,
});

// ===========================================================================
// The inactive session's default view
// ===========================================================================

describe("InactiveSessionActivities — the default view for a dormant session", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    appLocationPage.reset();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("shows the recorded activity transcript as the main view when an inactive session is selected", () => {
    // Given — a dormant session whose transcript the daemon can still replay
    const backend = aConnectionServiceBackend({
      sessions: [DORMANT],
      acpReplay: RECORDED_TRANSCRIPT,
    });

    // When
    mountScreen(backend);
    page.drawerItem(DORMANT.sessionId).click();

    // Then — the transcript is the pane, carrying the recorded entries in order
    sessionActivitiesPage.pane().should("exist");
    agentChatPage.chatMessage(0).should("have.text", "Rewrote the tokenizer.");
    agentChatPage.chatMessageKind(1).should("equal", "tool");
  });

  it("keeps the inspector closed when an inactive session is selected", () => {
    // Given
    const backend = aConnectionServiceBackend({
      sessions: [DORMANT],
      acpReplay: RECORDED_TRANSCRIPT,
    });

    // When — selection alone; no toggle click
    mountScreen(backend);
    page.drawerItem(DORMANT.sessionId).click();

    // Then
    page.inspectorDrawer().should("have.attr", "data-state", "closed");
  });

  it("offers Resume in the pane top bar for an inactive session", () => {
    // Given
    const backend = aConnectionServiceBackend({
      sessions: [DORMANT],
      acpReplay: RECORDED_TRANSCRIPT,
    });

    // When
    mountScreen(backend);
    page.drawerItem(DORMANT.sessionId).click();

    // Then — reachable without opening the inspector
    sessionActivitiesPage.resumeBtn(DORMANT.sessionId).should("be.visible");
  });

  it("resumes the session through the owning daemon when the top-bar Resume is clicked", () => {
    // Given
    const backend = aConnectionServiceBackend({
      sessions: [DORMANT],
      acpReplay: RECORDED_TRANSCRIPT,
    });

    // When
    mountScreen(backend);
    page.drawerItem(DORMANT.sessionId).click();
    sessionActivitiesPage.resume(DORMANT.sessionId);

    // Then — exactly one ResumeSession, for this session
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(ConnectionService.method.resumeSession);
      expect(calls.map((c) => c.sessionId), "ResumeSession calls").to.deep.equal([
        DORMANT.sessionId,
      ]);
    });
  });

  it("shows an explicit empty state when the inactive session recorded no activity", () => {
    // Given — a dormant session that never produced a transcript
    const backend = aConnectionServiceBackend({
      sessions: [DORMANT],
      acpReplay: NO_TRANSCRIPT,
    });

    // When
    mountScreen(backend);
    page.drawerItem(DORMANT.sessionId).click();

    // Then — the view still owns the pane, says so, and still offers Resume
    sessionActivitiesPage.empty().should("be.visible");
    agentChatPage.chatMessage(0, { timeout: 1000 }).should("not.exist");
    sessionActivitiesPage.resumeBtn(DORMANT.sessionId).should("be.visible");
  });

  it("makes no claim about recorded activity while the count feed is still pending", () => {
    // Given — a session whose count feed is subscribed but has not answered. Nothing yet
    // distinguishes "recorded nothing" from "not counted yet", and a feed that fails never answers
    // at all — so the same window covers a failed read.
    const { scenario, releaseCount } = aHeldCountReplay({ counts: [0], snapshot: [] });
    const backend = aConnectionServiceBackend({ sessions: [DORMANT], acpReplay: scenario });

    // When
    mountScreen(backend);
    page.drawerItem(DORMANT.sessionId).click();

    // Then — the pane is there, but says nothing it cannot support
    sessionActivitiesPage.pane().should("exist");
    sessionActivitiesPage.empty({ timeout: 1000 }).should("not.exist");

    // When — the feed finally reports a genuinely empty session
    cy.then(() => releaseCount());

    // Then — only now is the empty state a true statement
    sessionActivitiesPage.empty().should("be.visible");
  });

  it("shows the terminal and no Resume button for an active session", () => {
    // Given — a live session
    const backend = aConnectionServiceBackend({
      sessions: [LIVE],
      acpReplay: RECORDED_TRANSCRIPT,
      connectSession: {
        livekitRoom: "room-live",
        livekitUrl: "ws://127.0.0.1:7880",
        livekitServerIdentity: "server",
      },
    });

    // When
    mountScreen(backend);
    page.drawerItem(LIVE.sessionId).click();

    // Then — the terminal owns the pane; the dormant-session affordances are absent
    page.detailTerminalContainer().should("exist");
    sessionActivitiesPage.pane({ timeout: 1000 }).should("not.exist");
    sessionActivitiesPage.resumeBtn(LIVE.sessionId, { timeout: 1000 }).should("not.exist");
  });

  it("opens a deep-linked inspector tab for an inactive session", () => {
    // Given — a link that explicitly asks for the Details tab
    const backend = aConnectionServiceBackend({
      sessions: [DORMANT],
      acpReplay: RECORDED_TRANSCRIPT,
    });
    appLocationPage.startAt(`/sessions/${DORMANT.sessionId}?inspector=details`);

    // When
    mountScreen(backend);

    // Then — the explicit request still wins over the new closed-by-default rule, on the named tab
    page.inspectorDrawer().should("have.attr", "data-state", "open");
    page.inspectorDetailsTab().should("have.attr", "aria-selected", "true");
  });

  it("returns to the terminal view once the session becomes active", () => {
    // Given — a session the daemon reports dormant until the test says otherwise
    let sessionIsLive = false;
    const backend = aConnectionServiceBackend({
      listSessionsFactory: () => [
        sessionIsLive ? { ...DORMANT, isActive: true, status: "active" } : DORMANT,
      ],
      acpReplay: RECORDED_TRANSCRIPT,
      connectSession: {
        livekitRoom: "room-resumed",
        livekitUrl: "ws://127.0.0.1:7880",
        livekitServerIdentity: "server",
      },
    });

    // When — select it while dormant, then let the daemon report it alive
    mountScreen(backend);
    page.drawerItem(DORMANT.sessionId).click();
    sessionActivitiesPage.pane().should("exist");
    cy.then(() => {
      sessionIsLive = true;
    });

    // Then — the pane swaps back to the terminal, Resume goes away, and exactly one attach was taken
    page.detailTerminalContainer().should("exist");
    sessionActivitiesPage.pane({ timeout: 1000 }).should("not.exist");
    sessionActivitiesPage.resumeBtn(DORMANT.sessionId, { timeout: 1000 }).should("not.exist");
    cy.wrap(null).should(() => {
      const attached = backend
        .callsTo(ConnectionService.method.connectSession)
        .map((c) => c.sessionId);
      expect(attached, "ConnectSession calls").to.deep.equal([DORMANT.sessionId]);
    });
  });

  it("re-attaches a session resumed a second time within the same selection", () => {
    // Given — a session that dies again after its first resume, with the selection never changing.
    // The attach claim is per live epoch, so the second revival owes a second ConnectSession; a
    // claim that survived the death in between would strand the pane on an empty placeholder.
    let sessionIsLive = false;
    const backend = aConnectionServiceBackend({
      listSessionsFactory: () => [
        sessionIsLive ? { ...DORMANT, isActive: true, status: "active" } : DORMANT,
      ],
      acpReplay: RECORDED_TRANSCRIPT,
      connectSession: {
        livekitRoom: "room-resumed",
        livekitUrl: "ws://127.0.0.1:7880",
        livekitServerIdentity: "server",
      },
    });

    // When — live, dead, live again
    mountScreen(backend);
    page.drawerItem(DORMANT.sessionId).click();
    sessionActivitiesPage.pane().should("exist");
    cy.then(() => {
      sessionIsLive = true;
    });
    page.detailTerminalContainer().should("exist");
    cy.then(() => {
      sessionIsLive = false;
    });
    sessionActivitiesPage.pane().should("exist");
    cy.then(() => {
      sessionIsLive = true;
    });

    // Then — the terminal comes back, which takes a second attach
    page.detailTerminalContainer().should("exist");
    cy.wrap(null).should(() => {
      const attached = backend
        .callsTo(ConnectionService.method.connectSession)
        .map((c) => c.sessionId);
      expect(attached, "ConnectSession calls").to.deep.equal([
        DORMANT.sessionId,
        DORMANT.sessionId,
      ]);
    });
  });

  it("does not attach twice when the session is resumed from the top bar", () => {
    // Given — a dormant session whose ResumeSession already returns LiveKit coordinates, and which
    // the daemon reports alive once the resume has spawned it
    let sessionIsLive = false;
    const backend = aConnectionServiceBackend({
      listSessionsFactory: () => [
        sessionIsLive ? { ...DORMANT, isActive: true, status: "active" } : DORMANT,
      ],
      acpReplay: RECORDED_TRANSCRIPT,
      resumeSession: {
        livekitRoom: "room-resumed",
        livekitUrl: "ws://127.0.0.1:7880",
        livekitServerIdentity: "server",
      },
      connectSession: {
        livekitRoom: "room-connected",
        livekitUrl: "ws://127.0.0.1:7880",
        livekitServerIdentity: "server",
      },
    });

    // When — resume from the top bar, then let the list catch up and report the session live. The
    // base view follows the list, not the attachment, so the terminal appears on that second step.
    mountScreen(backend);
    page.drawerItem(DORMANT.sessionId).click();
    sessionActivitiesPage.resume(DORMANT.sessionId);
    cy.wrap(null).should(() => {
      expect(backend.callsTo(ConnectionService.method.resumeSession)).to.have.length(1);
    });
    cy.then(() => {
      sessionIsLive = true;
    });
    page.detailTerminalContainer().should("exist");

    // Then — the resume itself was the attach; the liveness poll did not pile a ConnectSession on
    // behind it, which would have minted a second browser identity and bounced the terminal
    // through a reconnect and another ClaimTerminalControl
    cy.wrap(null).should(() => {
      const resumed = backend
        .callsTo(ConnectionService.method.resumeSession)
        .map((c) => c.sessionId);
      expect(resumed, "ResumeSession calls").to.deep.equal([DORMANT.sessionId]);
      expect(backend.callsTo(ConnectionService.method.connectSession), "ConnectSession calls").to
        .be.empty;
    });
  });
});

// ===========================================================================
// Base-view precedence — workflow views are not replaced
// ===========================================================================

describe("InactiveSessionActivities — workflow views keep precedence", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("keeps the planned-PR view for an inactive pr-stack session", () => {
    // Given — a dormant PR-Stack orchestrator
    const client = aReplayClient();

    // When
    mountWithRpc(
      <SessionMainPane
        // No host connection in scope: this spec is not about the inspector's media tabs,
        // and `host` is required so that saying so is a choice rather than an omission.
        host={null}
        {...noopHandlers}
        selectedSession={DORMANT_PR_STACK as unknown as SessionEntry}
        attachment={{ status: "idle" } satisfies SessionAttachmentState}
        inspectorState="closed"
        client={client}
      />,
      anInMemoryRpcBackend(),
    );

    // Then — the planned-PR control surface survives dormancy
    prStackScreenPage.screen().should("exist");
    sessionActivitiesPage.pane({ timeout: 1000 }).should("not.exist");
  });

  it("offers Resume for an inactive pr-stack session whose view is left alone", () => {
    // Given — the same dormant orchestrator: Resume is keyed on liveness, not on the base view
    const client = aReplayClient();

    // When
    mountWithRpc(
      <SessionMainPane
        host={null}
        {...noopHandlers}
        selectedSession={DORMANT_PR_STACK as unknown as SessionEntry}
        attachment={{ status: "idle" } satisfies SessionAttachmentState}
        inspectorState="closed"
        client={client}
      />,
      anInMemoryRpcBackend(),
    );

    // Then — same top-bar position as a dormant terminal session gets
    sessionActivitiesPage.resumeBtn(DORMANT_PR_STACK.sessionId).should("be.visible");
  });

  it("keeps the workflow chat view for an inactive workflow session", () => {
    // Given — a dormant tddy-coder workflow session
    const client = aReplayClient();

    // When
    mountWithRpc(
      <SessionMainPane
        host={null}
        {...noopHandlers}
        selectedSession={DORMANT_WORKFLOW as unknown as SessionEntry}
        attachment={{ status: "idle" } satisfies SessionAttachmentState}
        inspectorState="closed"
        client={client}
      />,
      anInMemoryRpcBackend(),
    );

    // Then — its own chat screen renders, not the activities view
    workflowChatScreenPage.screen().should("exist");
    sessionActivitiesPage.pane({ timeout: 1000 }).should("not.exist");
  });
});

// ===========================================================================
// The activities view and the agent-activity overlay
// ===========================================================================

describe("InactiveSessionActivities — one transcript per pane", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("hides the agent activity overlay icon while the activities view is showing", () => {
    // Given — a dormant terminal session with a recorded transcript
    const client = aReplayClient();

    // When
    mountWithRpc(
      <SessionMainPane
        host={null}
        {...noopHandlers}
        selectedSession={DORMANT as unknown as SessionEntry}
        attachment={{ status: "idle" } satisfies SessionAttachmentState}
        inspectorState="closed"
        client={client}
      />,
      anInMemoryRpcBackend(),
    );

    // Then — the transcript is the pane, so the popover that shows the same thing is suppressed.
    // Waiting for a rendered entry first is what makes this meaningful: the overlay icon only
    // appears once the count feed has delivered, so asserting its absence before then would pass
    // against a suppression that does not exist.
    agentChatPage.chatMessage(1).should("exist");
    agentActivityPage.button({ timeout: 1000 }).should("not.exist");
  });

  it("keeps the agent activity overlay icon for an inactive pr-stack session", () => {
    // Given — a dormant session whose base view is NOT the activities view
    const client = aReplayClient();

    // When
    mountWithRpc(
      <SessionMainPane
        host={null}
        {...noopHandlers}
        selectedSession={DORMANT_PR_STACK as unknown as SessionEntry}
        attachment={{ status: "idle" } satisfies SessionAttachmentState}
        inspectorState="closed"
        client={client}
      />,
      anInMemoryRpcBackend(),
    );

    // Then — the overlay is still the only way to read the transcript here
    agentActivityPage.button().should("exist");
  });

  it("opens the tool call detail dialog from an entry in the activities view", () => {
    // Given — a transcript whose tool call has recorded bodies
    const client = aReplayClient();

    // When
    mountWithRpc(
      <SessionMainPane
        host={null}
        {...noopHandlers}
        selectedSession={DORMANT as unknown as SessionEntry}
        attachment={{ status: "idle" } satisfies SessionAttachmentState}
        inspectorState="closed"
        client={client}
      />,
      anInMemoryRpcBackend(),
    );
    sessionActivitiesPage.openToolDetail(1);

    // Then — the shared detail dialog resolves the body by tool_call_id
    agentActivityPage.detailDialog().should("be.visible");
    agentActivityPage.detailInput().should("contain.text", "tokenizer.rs");
  });

  it("keeps a mounted runtime unfocused behind the activities view", () => {
    // Given — a dormant session that still has a runtime mounted from an earlier attach
    const transport = aReplayTransportWithHeldTerminal();

    // When
    mountWithRpc(
      <SessionMainPane
        host={null}
        {...noopHandlers}
        selectedSession={DORMANT as unknown as SessionEntry}
        attachment={{ status: "idle" } satisfies SessionAttachmentState}
        inspectorState="closed"
        client={createClient(ConnectionService, transport)}
        runtimes={[aHostServedRuntimeFor(DORMANT.sessionId, transport)]}
        focusedRuntimeId={DORMANT.sessionId}
      />,
      anInMemoryRpcBackend(),
    );

    // Then — the runtime layer stays mounted (background streaming preserved) but nothing is
    // foregrounded over the transcript, so no claim-terminal CTA appears
    sessionActivitiesPage.pane().should("exist");
    page.runtimeLayer().should("exist");
    page.detailTerminalContainer({ timeout: 1000 }).should("not.exist");
    page.terminalControlOverlay({ timeout: 1000 }).should("not.exist");
  });
});
