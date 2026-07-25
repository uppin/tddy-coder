/**
 * Behaviour spec: dragging files onto the terminal uploads them to the session
 * dir on the host, then types the uploaded files' absolute host paths into the
 * terminal input — emulating a native terminal file-drag.
 *
 * Fails today: there is no `TerminalFileDropZone`, no `UploadSessionFileChunk`
 * RPC, and nothing types an uploaded path into the terminal.
 *
 * PRD: docs/ft/web/web-terminal.md § File drop upload
 */

import React from "react";
import { TerminalFileDropZone } from "../../src/components/connection/TerminalFileDropZone";
import { UploadProgressProvider } from "../../src/rpc/uploadProgress";
import { ConnectionService } from "../../src/gen/connection_pb";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { terminalFileUploadPage as page } from "../support/pages/terminalFileUploadPage";
import { aFile, dragOverWith, dropFilesOnto, reconstructUtf8 } from "../support/util/fileDrop";

const SESSION_ID = "s1";
const SESSION_TOKEN = "tok-1";

/** A backend that echoes each file's chosen host path on the final chunk. */
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

/** Bytes recorded for `fileName`, ordered by call sequence, reassembled to a string. */
function uploadedContents(backend: InMemoryRpcBackend, fileName: string): string {
  const chunks = backend
    .callsTo(ConnectionService.method.uploadSessionFileChunk)
    .filter((c) => c.fileName === fileName)
    .map((c) => c.data as Uint8Array);
  return reconstructUtf8(chunks);
}

describe("Terminal file drop — upload and type the host path", () => {
  it("shows a drop overlay while a file is dragged over the terminal", () => {
    const insertInput = cy.stub().as("insertInput");
    mountDropZone(anUploadBackend(), insertInput);

    // Given — no overlay before dragging
    page.dropOverlay({ timeout: 100 }).should("not.exist");

    // When — a file is dragged over the terminal
    dragOverWith(page.dropZoneSelector, [aFile("note.txt", "hi")]);

    // Then — the drop overlay appears
    page.dropOverlay().should("exist");
  });

  it("uploads each dropped file's bytes to the host under one drop id", () => {
    const backend = anUploadBackend();
    mountDropZone(backend, cy.stub().as("insertInput"));

    // When — two files are dropped at once
    dropFilesOnto(page.dropZoneSelector, [
      aFile("alpha.txt", "AAAA"),
      aFile("beta.txt", "BBBBBB"),
    ]);

    // Then — the daemon received each file's exact bytes, grouped under a single upload id
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(ConnectionService.method.uploadSessionFileChunk);
      const uploadIds = new Set(calls.map((c) => c.uploadId));
      expect(uploadIds.size, "all chunks share one drop/upload id").to.equal(1);
      // The upload id is a UUID generated at runtime, so only its presence can be asserted.
      expect([...uploadIds][0], "upload id is non-empty").to.have.length.greaterThan(0);
      calls.forEach((c) => expect(c.sessionId).to.equal(SESSION_ID));
      expect(uploadedContents(backend, "alpha.txt")).to.equal("AAAA");
      expect(uploadedContents(backend, "beta.txt")).to.equal("BBBBBB");
    });
  });

  it("marks the final chunk of each file so the host path can be returned", () => {
    const backend = anUploadBackend();
    mountDropZone(backend, cy.stub().as("insertInput"));

    dropFilesOnto(page.dropZoneSelector, [aFile("solo.txt", "x")]);

    cy.wrap(null).should(() => {
      const finals = backend
        .callsTo(ConnectionService.method.uploadSessionFileChunk)
        .filter((c) => c.fileName === "solo.txt" && c.last);
      expect(finals, "exactly one final chunk per file").to.have.length(1);
    });
  });

  it("types the uploaded files' escaped host paths, space-separated with a trailing space and no newline", () => {
    mountDropZone(anUploadBackend(), cy.stub().as("insertInput"));

    dropFilesOnto(page.dropZoneSelector, [
      aFile("alpha.txt", "AAAA"),
      aFile("beta.txt", "BBBBBB"),
    ]);

    // Then — the terminal input receives both host paths, quoted, in drop order, with one
    // trailing space and no Enter (matches native terminal drag).
    cy.get("@insertInput").should((subject) => {
      const stub = subject as unknown as sinon.SinonStub;
      expect(stub.callCount, "paths inserted as one run").to.equal(1);
      expect(stub.firstCall.args[0]).to.equal(
        "'/srv/host/uploads/alpha.txt' '/srv/host/uploads/beta.txt' ",
      );
    });
  });
});
