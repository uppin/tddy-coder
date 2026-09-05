/**
 * Behaviour spec: `SessionMainPane` renders the real Ghostty terminal for a session whose
 * connection carries tracks, fed by that connection.
 *
 * Such a session used to render only a static placeholder ("Terminal connected to {room}") — this
 * is the only attachment path tddy-coder recipe sessions (e.g. `plan-pr-stack`) ever reach, since
 * `connect_session` always returns a LiveKit room for any session type other than `claude-cli` /
 * `workspace`.
 *
 * Where the bytes come from is the pane's whole part in this: it opens the terminal on the
 * session's connection, states what the connection cannot know, and renders what arrives. It mints
 * no token and joins no room — the connection did both when the session was attached, which is why
 * there is one handshake here now instead of two.
 *
 * Changeset: `docs/dev/1-WIP/2026-09-05-optional-livekit-terminal-convergence.md`.
 */

import React from "react";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { SessionMainPane } from "../../src/components/sessions/SessionMainPane";
import type { SessionAttachmentState } from "../../src/components/sessions/useSessionAttachment";
import { aSessionConnection } from "../support/rpc/sessionConnections";
import { byTestId, TEST_IDS } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const FAKE_SESSION = {
  sessionId: "livekit-terminal-test-aaaa-0000-0000-000000000001",
  createdAt: "2026-06-30T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/my-recipe-session",
  pid: 43001,
  isActive: true,
  projectId: "proj-livekit-terminal-1",
  daemonInstanceId: "",
  workflowGoal: "plan-pr-stack recipe session",
  pendingElicitation: false,
};

/** The signed-in operator's daemon access token — what the daemon refuses to serve a terminal without. */
const SESSION_TOKEN = "an-operator-access-token";

/**
 * A session carried over its own room, serving a terminal.
 *
 * Rebuilt per spec: the connection records every `openTerminal` it is asked for, and a shared one
 * would carry the previous spec's opens into the next.
 */
function aRoomCarriedSession() {
  return aSessionConnection(FAKE_SESSION.sessionId)
    .carriedByRoom("tddy-lobby", {
      url: "ws://localhost:9999",
      serverIdentity: "daemon-dev-livekit-terminal-test-0001",
    })
    .servingOver(anInMemoryRpcBackend().transport())
    .servingTerminal();
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

function mountMainPaneFor(attached: ReturnType<typeof aRoomCarriedSession>) {
  const connection = attached.build();
  const attachment: SessionAttachmentState = { status: "connected", connection };
  const hint = attached.buildHint();

  cy.mount(
    <SessionMainPane
      // No host connection in scope: this spec is not about the inspector's media tabs,
      // and `host` is required so that saying so is a choice rather than an omission.
      host={null}
      selectedSession={FAKE_SESSION as never}
      attachment={attachment}
      attachmentHint={hint}
      inspectorState="closed"
      onToggleInspector={cy.stub()}
      onInspectorClose={cy.stub()}
      onInspectorExpand={cy.stub()}
      onInspectorRestore={cy.stub()}
      onResume={cy.stub()}
      onDelete={cy.stub()}
      onTerminate={cy.stub()}
      sessionToken={SESSION_TOKEN}
      runtimes={[
        {
          sessionId: FAKE_SESSION.sessionId,
          attached: true,
          connection,
          hint,
          bytesIn: 0,
          bytesOut: 0,
          lastDataReceivedAt: null,
        },
      ]}
      focusedRuntimeId={FAKE_SESSION.sessionId}
    />,
  );
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("SessionMainPane — a room-carried session renders a real terminal", () => {
  it("renders the Ghostty terminal for a room-carried session", () => {
    // Given / When
    mountMainPaneFor(aRoomCarriedSession());

    // Then
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");
  });

  it("does not render the static 'Terminal connected to' placeholder once a terminal is wired", () => {
    // Given / When
    mountMainPaneFor(aRoomCarriedSession());
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");

    // Then
    byTestId(TEST_IDS.sessionsDetailPane).should("not.contain.text", "Terminal connected to");
  });

  it("opens the terminal on the session's own connection, for the session's Agent terminal", () => {
    // Given a session carried over a room
    const attached = aRoomCarriedSession();

    // When the pane mounts
    mountMainPaneFor(attached);
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");

    // Then it asked the connection for one terminal — the reserved Agent one, named by the empty
    // id — rather than joining a room or minting a token of its own
    cy.wrap(null).should(() => {
      expect(attached.terminalOpens).to.have.length(1);
      expect(attached.terminalOpens[0].terminalId ?? "").to.equal("");
    });
  });

  it("states the signed-in operator's session token, which the daemon refuses a terminal without", () => {
    // Given a session carried over a room
    const attached = aRoomCarriedSession();

    // When the pane mounts
    mountMainPaneFor(attached);
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");

    // Then the operator's token went with the open. Both terminal RPCs are server-streaming, and
    // the transport's auth gate is unary-only — a terminal that left this to the gate would open a
    // stream the daemon refuses as unauthenticated
    cy.wrap(null).should(() => {
      expect(attached.terminalOpens[0].sessionToken).to.equal(SESSION_TOKEN);
    });
  });

  it("renders the output arriving on the connection's feed", () => {
    // Given a mounted terminal
    const attached = aRoomCarriedSession();
    mountMainPaneFor(attached);
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");

    // When the session's process writes
    cy.wrap(null).then(() => attached.deliverToTerminal("recipe session ready"));

    // Then the terminal painted it — the feed is the whole of what the pane needed
    byTestId("terminal-buffer-text", { timeout: 10000 }).should(
      "contain.text",
      "recipe session ready",
    );
  });

  it("does not show a visible 'connecting'/'connected' status strip above the terminal", () => {
    // Given / When
    mountMainPaneFor(aRoomCarriedSession());
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");

    // Then — the raw status strip stays in the DOM (for tooling) but must not be visible to the
    // user: the drawer shows the session's connection state on its own overlay
    byTestId(TEST_IDS.livekitStatus).should("exist").and("not.be.visible");
  });
});
