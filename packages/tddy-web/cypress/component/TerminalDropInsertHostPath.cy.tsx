/**
 * Behaviour spec: dropping an already-uploaded file (dragged out of the Files tab, carrying its
 * host path under the private MIME) onto the terminal inserts the quoted host path into the
 * terminal input WITHOUT re-uploading — the bytes are already on the host.
 *
 * PRD: docs/ft/web/session-files-inspector.md
 */

import React from "react";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ConnectionService } from "../../src/gen/connection_pb";
import { TerminalFileDropZone } from "../../src/components/connection/TerminalFileDropZone";
import { UploadProgressProvider } from "../../src/rpc/uploadProgress";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { terminalFileUploadPage as page } from "../support/pages/terminalFileUploadPage";
import { dragOverWithHostPath, dropHostPathOnto } from "../support/util/fileDrop";

const SESSION_ID = "drop-insert-session-1";
const SESSION_TOKEN = "tok-drop-insert";
const HOST_PATH = "/srv/host/sessions/drop-insert-session-1/uploads/upload-aaaa/report.pdf";

/** A backend that would record any upload chunk — used to prove none is sent for an internal drag. */
function anUploadBackend(): InMemoryRpcBackend {
  return anInMemoryRpcBackend().onUnary(
    ConnectionService.method.uploadSessionFileChunk,
    (req) => ({ hostPath: req.last ? `/srv/host/uploads/${req.fileName}` : "" }),
  );
}

function mountDropZone(backend: InMemoryRpcBackend, insertInput: Cypress.Agent<sinon.SinonStub>) {
  mountWithRpc(
    withSelectedDaemon(
      <UploadProgressProvider>
        <TerminalFileDropZone
          sessionToken={SESSION_TOKEN}
          sessionId={SESSION_ID}
          insertInput={insertInput}
        >
          <div data-testid="ghostty-terminal" style={{ width: 400, height: 300 }} />
        </TerminalFileDropZone>
      </UploadProgressProvider>,
    ),
    backend,
  );
}

describe("Terminal drop — insert an already-uploaded host path without re-uploading", () => {
  it("shows the drop overlay while an internal host-path drag is over the terminal", () => {
    mountDropZone(anUploadBackend(), cy.stub().as("insertInput"));

    // Given — no overlay before dragging
    page.dropOverlay({ timeout: 100 }).should("not.exist");

    // When — an internal host-path drag is over the terminal
    dragOverWithHostPath(page.dropZoneSelector, HOST_PATH);

    // Then — the same drop overlay appears
    page.dropOverlay().should("exist");
  });

  it("inserts the quoted host path and uploads nothing when an internal drag is dropped", () => {
    const backend = anUploadBackend();
    mountDropZone(backend, cy.stub().as("insertInput"));

    // When — a file dragged out of the Files tab is dropped on the terminal
    dropHostPathOnto(page.dropZoneSelector, HOST_PATH);

    // Then — the quoted host path is typed with a trailing space and no newline ...
    cy.get("@insertInput").should((subject) => {
      const stub = subject as unknown as sinon.SinonStub;
      expect(stub.callCount, "path inserted as one run").to.equal(1);
      expect(stub.firstCall.args[0]).to.equal(`'${HOST_PATH}' `);
    });

    // ... and no upload chunk was sent (the file is already on the host)
    cy.wrap(null).should(() => {
      expect(backend.callsTo(ConnectionService.method.uploadSessionFileChunk)).to.have.length(0);
    });
  });
});
