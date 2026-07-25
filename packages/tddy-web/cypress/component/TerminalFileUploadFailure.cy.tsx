/**
 * Behaviour spec: when one file in a multi-file drop fails to upload, that file
 * is skipped — its path is not typed — and the failure is surfaced, while the
 * remaining files still upload and get typed.
 *
 * Fails today: no upload orchestration, no per-file error surface.
 *
 * PRD: docs/ft/web/web-terminal.md § File drop upload (failures are surfaced, not fatal)
 */

import React from "react";
import { ConnectError, Code } from "@connectrpc/connect";
import { TerminalFileDropZone } from "../../src/components/connection/TerminalFileDropZone";
import { UploadProgressProvider } from "../../src/rpc/uploadProgress";
import { UploadProgressIndicator } from "../../src/components/sessions/UploadProgressIndicator";
import { ConnectionService } from "../../src/gen/connection_pb";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { terminalFileUploadPage as page } from "../support/pages/terminalFileUploadPage";
import { aFile, dropFilesOnto } from "../support/util/fileDrop";

/** A backend that fails every chunk of `bad.txt` and echoes a host path for anything else. */
function aPartiallyFailingBackend(): InMemoryRpcBackend {
  return anInMemoryRpcBackend().onUnary(
    ConnectionService.method.uploadSessionFileChunk,
    (req) => {
      if (req.fileName === "bad.txt") {
        throw new ConnectError("disk error", Code.Internal);
      }
      return { hostPath: req.last ? `/srv/host/uploads/${req.fileName}` : "" };
    },
  );
}

function mountDropZoneWithIndicator(insertInput: Cypress.Agent<sinon.SinonStub>) {
  mountWithRpc(
    withSelectedDaemon(
      <UploadProgressProvider>
        <TerminalFileDropZone sessionToken="tok" sessionId="s1" insertInput={insertInput}>
          <div data-testid="ghostty-terminal" style={{ width: 400, height: 300 }} />
        </TerminalFileDropZone>
        <UploadProgressIndicator />
      </UploadProgressProvider>,
    ),
    aPartiallyFailingBackend(),
  );
}

describe("Terminal file upload — a failed file is skipped, others proceed", () => {
  it("types only the successfully uploaded file's host path", () => {
    const insertInput = cy.stub().as("insertInput");
    mountDropZoneWithIndicator(insertInput);

    // When — a good and a failing file are dropped together
    dropFilesOnto(page.dropZoneSelector, [aFile("good.txt", "ok"), aFile("bad.txt", "nope")]);

    // Then — only the good file's path is typed (the failed file is skipped, trailing space kept)
    cy.get("@insertInput").should((subject) => {
      const stub = subject as unknown as sinon.SinonStub;
      expect(stub.callCount).to.equal(1);
      expect(stub.firstCall.args[0]).to.equal("'/srv/host/uploads/good.txt' ");
    });
  });

  it("surfaces the failed file by name in the progress error", () => {
    mountDropZoneWithIndicator(cy.stub().as("insertInput"));

    dropFilesOnto(page.dropZoneSelector, [aFile("good.txt", "ok"), aFile("bad.txt", "nope")]);

    // Then — the failure is surfaced, naming the skipped file
    page.progressError().should("contain.text", "bad.txt");
  });
});
