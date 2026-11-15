/**
 * Acceptance: renaming a session from the inspector's Details tab asks the session's owning host
 * (`SessionService.RenameSession` on the drawer's `client`, which is already that host's) and then
 * asks the sessions list to refresh, so the new name reaches the drawer row. A host's refusal is
 * shown beside the name. With no client — nothing reaches the host — the name is shown without a
 * way to edit it.
 */

import React from "react";
import { Code, createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { create } from "@bufbuild/protobuf";
import {
  RenameSessionResponseSchema,
  SessionService,
  type SessionEntry,
} from "tddy-client-web/gen/session_pb";
import { SessionInspectorDrawer } from "../../src/SessionInspectorDrawer";
import { SESSIONS_REFRESH_EVENT } from "../../src/lib/sessionsRefreshBridge";
import {
  byTestId,
  sessionNameError,
  sessionNameInput,
  sessionNameLabel,
  sessionNameRenameBtn,
} from "../support/testIds";

const SESSION_TOKEN = "tok-inspector-rename";
const NAME = "Fix the login flow";
const REFUSAL = "display name must not contain control characters";

const SESSION = {
  sessionId: "inspector-rename-session-1",
  createdAt: "2026-11-15T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/my-feature-branch",
  pid: 4242,
  isActive: true,
  projectId: "proj-rename-1",
  daemonInstanceId: "host-b",
  displayName: "",
  pendingElicitation: false,
};

/** The session's host, accepting every rename as the daemon would — trimmed. */
function aHostAcceptingRenames(): InMemoryRpcBackend {
  return anInMemoryRpcBackend().onUnary(SessionService.method.renameSession, (req) =>
    create(RenameSessionResponseSchema, { displayName: req.displayName.trim() }),
  );
}

function aHostRefusingRenames(): InMemoryRpcBackend {
  return anInMemoryRpcBackend().failWith(
    SessionService.method.renameSession,
    Code.InvalidArgument,
    REFUSAL,
  );
}

function theInspectorOn(backend: InMemoryRpcBackend, options: { withClient: boolean }) {
  const noop = () => undefined;
  cy.mountWithClients(
    <SessionInspectorDrawer
      // Not about the media tabs; `hostConnection` is required so saying so is a choice.
      hostConnection={null}
      state="open"
      session={SESSION as unknown as SessionEntry}
      onClose={noop}
      onExpand={noop}
      onRestore={noop}
      onResume={noop}
      onDelete={noop}
      onTerminate={noop}
      client={options.withClient ? createClient(SessionService, backend.transport()) : undefined}
      sessionToken={SESSION_TOKEN}
    />,
    backend,
  );
}

/** Records every sessions-list refresh request raised in the app's window. */
function listenForSessionsRefresh() {
  const refreshed = cy.stub().as("sessionsRefresh");
  cy.window().then((win) => win.addEventListener(SESSIONS_REFRESH_EVENT, refreshed));
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

describe("Session inspector — renaming a session", () => {
  it("renames the session on its host and refreshes the sessions list", () => {
    // Given
    const backend = aHostAcceptingRenames();
    theInspectorOn(backend, { withClient: true });
    listenForSessionsRefresh();

    // When
    byTestId(sessionNameRenameBtn(SESSION.sessionId)).click();
    byTestId(sessionNameInput(SESSION.sessionId)).type(`${NAME}{enter}`);

    // Then the host was asked once, for this session, with the operator's token
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(SessionService.method.renameSession);
      expect(calls).to.have.length(1);
      expect(calls[0].sessionToken).to.eq(SESSION_TOKEN);
      expect(calls[0].sessionId).to.eq(SESSION.sessionId);
      expect(calls[0].displayName).to.eq(NAME);
    });
    // And the list was asked to pick the new name up
    cy.get("@sessionsRefresh").should("have.been.calledOnce");
    byTestId(sessionNameInput(SESSION.sessionId)).should("not.exist");
  });

  it("shows the host's refusal beside the name", () => {
    // Given
    theInspectorOn(aHostRefusingRenames(), { withClient: true });
    listenForSessionsRefresh();

    // When
    byTestId(sessionNameRenameBtn(SESSION.sessionId)).click();
    byTestId(sessionNameInput(SESSION.sessionId)).type(`${NAME}{enter}`);

    // Then
    byTestId(sessionNameError(SESSION.sessionId)).should("have.text", REFUSAL);
    byTestId(sessionNameInput(SESSION.sessionId)).should("have.value", NAME);
    cy.get("@sessionsRefresh").should("not.have.been.called");
  });

  it("without a client the name is shown with no way to rename it", () => {
    // When
    theInspectorOn(aHostAcceptingRenames(), { withClient: false });

    // Then
    byTestId(sessionNameLabel(SESSION.sessionId)).should("have.text", "my-feature-branch");
    byTestId(sessionNameRenameBtn(SESSION.sessionId)).should("not.exist");
  });
});
