/**
 * Behaviour spec: the Session Inspector exposes a Files tab, and the tab is wired so that starting
 * a drag from it closes the Inspector (the overlay must get out of the way so the terminal beneath
 * it becomes the drop target).
 *
 * PRD: docs/ft/web/session-files-inspector.md
 */

import React from "react";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ConnectionService, type SessionEntry } from "../../src/gen/connection_pb";
import { SessionInspectorDrawer } from "../../src/components/sessions/SessionInspectorDrawer";
import { mountWithRpc } from "../support/rpc/inMemory";
import { sessionFilesTabPage as page } from "../support/pages/sessionFilesTabPage";

const SESSION_ID = "inspector-files-session-1";
const SESSION_TOKEN = "tok-inspector-files";
const HOST_PATH = "/srv/host/sessions/inspector-files-session-1/uploads/upload-aaaa/report.pdf";

const SESSION = {
  sessionId: SESSION_ID,
  createdAt: "2026-07-25T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/feature-files",
  pid: 4242,
  isActive: true,
  projectId: "proj-files-1",
  daemonInstanceId: "local",
  pendingElicitation: false,
};

/** A backend whose Files list holds one uploaded file. */
function aBackendWithOneUpload(): InMemoryRpcBackend {
  return anInMemoryRpcBackend().onUnary(ConnectionService.method.listSessionUploads, () => ({
    uploads: [
      {
        uploadId: "upload-aaaa",
        fileName: "report.pdf",
        hostPath: HOST_PATH,
        sizeBytes: 2_000_000n,
        uploadedAtMs: 1_700_000_000_000n,
      },
    ],
  }));
}

function mountDrawer(backend: InMemoryRpcBackend, onClose: Cypress.Agent<sinon.SinonStub>) {
  const client = createClient(ConnectionService, backend.transport());
  const noop = () => undefined;
  mountWithRpc(
    <SessionInspectorDrawer
      state="open"
      session={SESSION as unknown as SessionEntry}
      onClose={onClose}
      onExpand={noop}
      onRestore={noop}
      onResume={noop}
      onDelete={noop}
      onTerminate={noop}
      client={client}
      sessionToken={SESSION_TOKEN}
    />,
    backend,
  );
}

describe("Session Inspector — Files tab wiring", () => {
  it("shows a Files tab that opens the uploaded-files list", () => {
    // Given — the inspector is open on a session with one uploaded file
    mountDrawer(aBackendWithOneUpload(), cy.stub().as("onClose"));

    // When — the Files tab is selected
    page.tabButton().click();

    // Then — the files panel renders that upload
    page.panel().should("be.visible");
    page.row("report.pdf").should("be.visible");
  });

  it("closes the Inspector when a file drag starts", () => {
    // Given — the inspector is open on the Files tab
    mountDrawer(aBackendWithOneUpload(), cy.stub().as("onClose"));
    page.tabButton().click();
    page.row("report.pdf").should("be.visible");

    // When — a drag begins on the file row
    page.row("report.pdf").trigger("dragstart", { dataTransfer: new DataTransfer(), force: true });

    // Then — the drawer's close handler fires
    cy.get("@onClose").should("have.been.calledOnce");
  });
});
