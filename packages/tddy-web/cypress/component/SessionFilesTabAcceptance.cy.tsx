/**
 * Behaviour spec: the Session Inspector → Files tab lists the files already uploaded to the
 * session and makes them repeatedly usable — insert the host path into the terminal, copy it,
 * delete the upload, or drag it onto the terminal. Starting a drag or a tap-to-insert closes the
 * Inspector so the terminal underneath becomes the drop target.
 *
 * PRD: docs/ft/web/session-files-inspector.md
 */

import React from "react";
import { createClient } from "@connectrpc/connect";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  ConnectionService,
  ListSessionUploadsResponseSchema,
  DeleteSessionUploadResponseSchema,
} from "../../src/gen/connection_pb";
import { SessionFilesTab } from "../../src/components/sessions/SessionFilesTab";
import { mountWithRpc } from "../support/rpc/inMemory";
import { sessionFilesTabPage as page } from "../support/pages/sessionFilesTabPage";
import { HOST_PATH_MIME } from "../support/util/fileDrop";

const SESSION_ID = "files-tab-session-1";
const SESSION_TOKEN = "tok-files-1";

interface Upload {
  uploadId: string;
  fileName: string;
  hostPath: string;
  sizeBytes: bigint;
  uploadedAtMs: bigint;
}

/** An uploaded-file entry with sensible defaults; override only what the scenario cares about. */
function anUpload(overrides: Partial<Upload> = {}): Upload {
  return {
    uploadId: "upload-aaaa",
    fileName: "report.pdf",
    hostPath: "/srv/host/sessions/files-tab-session-1/uploads/upload-aaaa/report.pdf",
    sizeBytes: 2_000_000n,
    uploadedAtMs: 1_700_000_000_000n,
    ...overrides,
  };
}

/**
 * A stateful backend: `ListSessionUploads` returns the current set, `DeleteSessionUpload` removes
 * the matching entry — so a delete followed by a reload drops the row (a fake, not a mock).
 */
function anUploadsBackend(initial: Upload[]): InMemoryRpcBackend {
  let uploads = [...initial];
  return anInMemoryRpcBackend()
    .onUnary(ConnectionService.method.listSessionUploads, () =>
      create(ListSessionUploadsResponseSchema, { uploads }),
    )
    .onUnary(ConnectionService.method.deleteSessionUpload, (req) => {
      uploads = uploads.filter(
        (u) => !(u.uploadId === req.uploadId && u.fileName === req.fileName),
      );
      return create(DeleteSessionUploadResponseSchema, {});
    });
}

function mountTab(
  backend: InMemoryRpcBackend,
  handlers: {
    onInsertPath: Cypress.Agent<sinon.SinonStub>;
    onCloseInspector: Cypress.Agent<sinon.SinonStub>;
  },
) {
  const client = createClient(ConnectionService, backend.transport());
  mountWithRpc(
    <SessionFilesTab
      client={client}
      sessionToken={SESSION_TOKEN}
      sessionId={SESSION_ID}
      onInsertPath={handlers.onInsertPath}
      onCloseInspector={handlers.onCloseInspector}
    />,
    backend,
  );
}

function stubHandlers() {
  return {
    onInsertPath: cy.stub().as("onInsertPath"),
    onCloseInspector: cy.stub().as("onCloseInspector"),
  };
}

describe("Session Inspector — Files tab", () => {
  it("lists the session's uploaded files with name and size", () => {
    // Given — the session has two uploaded files
    const backend = anUploadsBackend([
      anUpload({ fileName: "report.pdf", sizeBytes: 2_000_000n }),
      anUpload({ uploadId: "upload-bbbb", fileName: "diagram.png", sizeBytes: 340_000n }),
    ]);

    // When — the Files tab is shown
    mountTab(backend, stubHandlers());

    // Then — both files render with their formatted sizes
    page.row("report.pdf").should("be.visible");
    page.size("report.pdf").should("have.text", "2.0 MB");
    page.row("diagram.png").should("be.visible");
    page.size("diagram.png").should("have.text", "340.0 kB");
  });

  it("shows an empty state when the session has no uploads", () => {
    // Given — the session has no uploads
    const backend = anUploadsBackend([]);

    // When
    mountTab(backend, stubHandlers());

    // Then — the empty state renders and no file rows exist
    page.empty().should("be.visible");
    page.row("report.pdf", { timeout: 100 }).should("not.exist");
  });

  it("inserts the file's host path into the terminal and closes the Inspector on Insert", () => {
    // Given — one uploaded file
    const file = anUpload({ fileName: "report.pdf" });
    const handlers = stubHandlers();
    mountTab(anUploadsBackend([file]), handlers);

    // When — Insert is pressed on its row
    page.insert("report.pdf").click();

    // Then — the exact host path is inserted and the Inspector is closed once
    cy.get("@onInsertPath").should("have.been.calledOnceWith", file.hostPath);
    cy.get("@onCloseInspector").should("have.been.calledOnce");
  });

  it("copies the file's absolute host path to the clipboard on Copy path", () => {
    // Given — one uploaded file, with the clipboard write stubbed
    const file = anUpload({ fileName: "report.pdf" });
    mountTab(anUploadsBackend([file]), stubHandlers());
    cy.window().then((win) => {
      cy.stub(win.navigator.clipboard, "writeText").as("clipboardWrite").resolves();
    });

    // When — Copy path is pressed
    page.copyPath("report.pdf").click();

    // Then — the absolute host path is written to the clipboard
    cy.get("@clipboardWrite").should("have.been.calledOnceWith", file.hostPath);
  });

  it("deletes the upload only after the confirm step, then reloads the list", () => {
    // Given — one uploaded file
    const file = anUpload({ uploadId: "upload-aaaa", fileName: "report.pdf" });
    const backend = anUploadsBackend([file]);
    mountTab(backend, stubHandlers());
    page.row("report.pdf").should("be.visible");

    // When — Delete is pressed once (first step only)
    page.delete("report.pdf").click();

    // Then — nothing is deleted yet
    cy.wrap(null).should(() => {
      expect(backend.callsTo(ConnectionService.method.deleteSessionUpload)).to.have.length(0);
    });

    // When — the confirm step is pressed
    page.confirmDelete("report.pdf").click();

    // Then — exactly one delete for this file, addressed by upload id + name, and the row is gone
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(ConnectionService.method.deleteSessionUpload);
      expect(calls).to.have.length(1);
      expect(calls[0].uploadId).to.equal("upload-aaaa");
      expect(calls[0].fileName).to.equal("report.pdf");
    });
    page.row("report.pdf", { timeout: 5000 }).should("not.exist");
  });

  it("carries the file's host path and closes the Inspector when a drag starts", () => {
    // Given — one uploaded file
    const file = anUpload({ fileName: "report.pdf" });
    const handlers = stubHandlers();
    mountTab(anUploadsBackend([file]), handlers);

    // When — a drag begins on its row
    const dataTransfer = new DataTransfer();
    page.row("report.pdf").trigger("dragstart", { dataTransfer, force: true });

    // Then — the drag carries the host path under the private MIME, and the Inspector closes
    cy.wrap(null).should(() => {
      expect(dataTransfer.getData(HOST_PATH_MIME)).to.equal(file.hostPath);
    });
    cy.get("@onCloseInspector").should("have.been.calledOnce");
  });
});
