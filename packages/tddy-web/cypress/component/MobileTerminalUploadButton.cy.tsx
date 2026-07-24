/**
 * Behaviour spec: on mobile there is no OS drag-and-drop, so the upload gesture
 * is initiated from an "Attach" button in the Keyboard strip. Picking a file
 * runs the same upload → type-path flow as a desktop drop.
 *
 * Fails today: `GhosttyTerminalGrpc` renders no upload button, and there is no
 * `UploadSessionFileChunk` RPC nor path-typing.
 *
 * PRD: docs/ft/web/web-terminal.md § Mobile UX — File upload from the Keyboard strip
 */

import React from "react";
import { GhosttyTerminalGrpc, type GrpcStream } from "../../src/components/GhosttyTerminalGrpc";
import { UploadProgressProvider } from "../../src/rpc/uploadProgress";
import { ConnectionService } from "../../src/gen/connection_pb";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { TEST_IDS, byTestId } from "../support/testIds";
import { terminalFileUploadPage as page } from "../support/pages/terminalFileUploadPage";
import { reconstructUtf8 } from "../support/util/fileDrop";

function aCapturingStream(): GrpcStream {
  return {
    send: cy.stub().as("streamSend"),
    onMessage: () => {},
    close: () => {},
  };
}

function anUploadBackend(): InMemoryRpcBackend {
  return anInMemoryRpcBackend().onUnary(
    ConnectionService.method.uploadSessionFileChunk,
    (req) => ({ hostPath: req.last ? `/srv/host/uploads/${req.fileName}` : "" }),
  );
}

function mountMobileTerminal(stream: GrpcStream, backend: InMemoryRpcBackend) {
  cy.viewport(375, 667);
  mountWithRpc(
    withSelectedDaemon(
      <UploadProgressProvider>
        <div style={{ width: 375, height: 500, position: "relative" }}>
          <GhosttyTerminalGrpc
            sessionToken="tok"
            sessionId="s1"
            stream={stream}
          />
        </div>
      </UploadProgressProvider>,
    ),
    backend,
  );
}

/** Assert `stream.send` received the exact UTF-8 text of the inserted path run. */
function expectStreamReceivedText(text: string) {
  const expected = new TextEncoder().encode(text);
  cy.get("@streamSend").should((subject) => {
    const stub = subject as unknown as { getCalls: () => { args: unknown[] }[] };
    const received = stub.getCalls().some((call) => {
      const arg = call.args[0];
      return (
        arg instanceof Uint8Array &&
        arg.length === expected.length &&
        expected.every((b, i) => arg[i] === b)
      );
    });
    expect(received, `stream.send should receive "${text}"`).to.be.true;
  });
}

describe("Mobile terminal — upload from the Keyboard strip", () => {
  it("renders an Attach button beside the mobile Keyboard button", () => {
    mountMobileTerminal(aCapturingStream(), anUploadBackend());

    byTestId(TEST_IDS.mobileKeyboardButton).should("exist");
    page.uploadButton().should("exist");
  });

  it("uploads a file picked via the Attach button and types its host path into the terminal", () => {
    const backend = anUploadBackend();
    mountMobileTerminal(aCapturingStream(), backend);

    // When — the user picks a file from the native picker
    page.uploadFileInput().selectFile(
      { contents: Cypress.Buffer.from("hello"), fileName: "note.txt", mimeType: "text/plain" },
      { force: true },
    );

    // Then — the file's exact bytes are uploaded to the host
    cy.wrap(null).should(() => {
      const chunks = backend
        .callsTo(ConnectionService.method.uploadSessionFileChunk)
        .filter((c) => c.fileName === "note.txt")
        .map((c) => c.data as Uint8Array);
      expect(reconstructUtf8(chunks)).to.equal("hello");
    });

    // And — its escaped host path is typed into the terminal input (trailing space, no newline)
    expectStreamReceivedText("'/srv/host/uploads/note.txt' ");
  });
});
