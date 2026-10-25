/**
 * Behaviour spec: files dragged from Finder onto a session terminal inside Tddy Desktop.
 *
 * The desktop window's native drag-drop handler owns the drag, so the terminal never sees an HTML5
 * `drop`; the window reports the pointer position and the dropped paths instead. The terminal under
 * the pointer is highlighted while the drag is over it, and on drop:
 *
 * - a session whose agent reads this machine's files gets the **real paths**, shell-quoted and
 *   space-separated with a trailing space — exactly what a native terminal types for a drop;
 * - any other session (another host, a jail, a managed codebase) gets the files **uploaded** into
 *   its session directory, and the uploaded paths typed — what a browser drop does;
 * - what cannot be uploaded (a folder) is **refused with a visible message**.
 *
 * PRD: docs/ft/web/web-terminal.md § File drop upload
 */

import React from "react";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { SessionFilesService } from "../../src/gen/session_files_pb";
import { TerminalFileDropZone } from "../../src/components/connection/TerminalFileDropZone";
import { NativeFileDropProvider } from "../../src/components/connection/nativeFileDrop";
import { UploadProgressProvider } from "../../src/rpc/uploadProgress";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { terminalFileUploadPage as page } from "../support/pages/terminalFileUploadPage";
import {
  aNativeFileDropDouble,
  centreOf,
  type DroppedEntry,
  type NativeFileDropDouble,
} from "../support/util/nativeFileDrop";

const SESSION_ID = "native-drop-session-1";
const SESSION_TOKEN = "tok-native-drop";

const A_SCREENSHOT = "/Users/me/Desktop/Screen Shot 2026-10-25 at 10.00.00.png";
const A_QUOTED_NAME = "/Users/me/it's ✓ – notes.txt";
const A_FOLDER = "/Users/me/a folder";

const SCREENSHOT_BYTES = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 1, 2, 3]);
const NOTES_BYTES = new TextEncoder().encode("notes");

/** Where the host would store an upload, as the daemon answers on a file's last chunk. */
function uploadedPathOf(fileName: string): string {
  return `/home/remote/.tddy/sessions/${SESSION_ID}/uploads/u/${fileName}`;
}

function anUploadBackend(): InMemoryRpcBackend {
  return anInMemoryRpcBackend().onUnary(
    SessionFilesService.method.uploadSessionFileChunk,
    (req) => ({
      hostPath: req.last ? uploadedPathOf(req.fileName) : "",
    }),
  );
}

/** The bytes the backend received for `fileName`, chunks concatenated in order. */
function uploadedBytes(backend: InMemoryRpcBackend, fileName: string): number[] {
  return backend
    .callsTo(SessionFilesService.method.uploadSessionFileChunk)
    .filter((req) => req.fileName === fileName)
    .flatMap((req) => Array.from(req.data));
}

interface Terminal {
  testId: string;
  readsLocalPaths: boolean;
  insertInput: Cypress.Agent<sinon.SinonStub>;
}

function aTerminal(testId: string, readsLocalPaths: boolean): Terminal {
  return {
    testId,
    readsLocalPaths,
    insertInput: cy.stub().as(`insertInput-${testId}`),
  };
}

function mountTerminals(
  drop: NativeFileDropDouble,
  backend: InMemoryRpcBackend,
  terminals: Terminal[],
) {
  mountWithRpc(
    withSelectedDaemon(
      <UploadProgressProvider>
        <NativeFileDropProvider bridge={drop.bridge}>
          <div style={{ display: "flex", gap: 16 }}>
            {terminals.map((terminal) => (
              <div key={terminal.testId} style={{ width: 300, height: 240 }}>
                <TerminalFileDropZone
                  sessionToken={SESSION_TOKEN}
                  sessionId={SESSION_ID}
                  insertInput={terminal.insertInput}
                  readsLocalPaths={terminal.readsLocalPaths}
                >
                  <div data-testid={terminal.testId} style={{ width: "100%", height: "100%" }} />
                </TerminalFileDropZone>
              </div>
            ))}
            <div data-testid="not-a-terminal" style={{ width: 200, height: 240 }} />
          </div>
        </NativeFileDropProvider>
      </UploadProgressProvider>,
    ),
    backend,
  );
}

function selector(testId: string): string {
  return `[data-testid='${testId}']`;
}

/** Drag `paths` in from the OS and drop them on the element `testId` names. */
function dropOn(drop: NativeFileDropDouble, testId: string, paths: string[]) {
  centreOf(selector(testId)).then((point) => {
    drop.emit({ type: "enter", paths, point });
    drop.emit({ type: "over", point });
    drop.emit({ type: "drop", paths, point });
  });
}

function aDropOf(files: Record<string, DroppedEntry>): NativeFileDropDouble {
  return aNativeFileDropDouble(files);
}

describe("Terminal native file drop (Tddy Desktop)", () => {
  it("highlights the terminal a drag from the OS is over, and clears it when the drag leaves", () => {
    // Given
    const drop = aDropOf({ [A_SCREENSHOT]: SCREENSHOT_BYTES });
    mountTerminals(drop, anUploadBackend(), [aTerminal("terminal-a", true)]);
    page.dropOverlay({ timeout: 100 }).should("not.exist");

    // When a drag enters over the terminal
    centreOf(selector("terminal-a")).then((point) => {
      drop.emit({ type: "enter", paths: [A_SCREENSHOT], point });
      drop.emit({ type: "over", point });
    });

    // Then it is highlighted as a drop target
    page.dropOverlay().should("exist");

    // When the drag leaves the window
    cy.then(() => drop.emit({ type: "leave" }));

    // Then the highlight clears
    page.dropOverlay({ timeout: 2000 }).should("not.exist");
  });

  it("does not highlight a terminal the drag is not over", () => {
    // Given
    const drop = aDropOf({ [A_SCREENSHOT]: SCREENSHOT_BYTES });
    mountTerminals(drop, anUploadBackend(), [aTerminal("terminal-a", true)]);

    // When a drag hovers over something that is not a terminal
    centreOf(selector("not-a-terminal")).then((point) => {
      drop.emit({ type: "enter", paths: [A_SCREENSHOT], point });
      drop.emit({ type: "over", point });
    });

    // Then nothing is highlighted
    page.dropOverlay({ timeout: 300 }).should("not.exist");
  });

  it("types the real paths, shell-quoted, into a session that reads this machine's files", () => {
    // Given a session whose agent reads this machine's files
    const drop = aDropOf({
      [A_SCREENSHOT]: SCREENSHOT_BYTES,
      [A_QUOTED_NAME]: NOTES_BYTES,
    });
    const backend = anUploadBackend();
    const terminal = aTerminal("terminal-a", true);
    mountTerminals(drop, backend, [terminal]);

    // When two files are dropped on its terminal
    dropOn(drop, "terminal-a", [A_SCREENSHOT, A_QUOTED_NAME]);

    // Then both paths are typed as one run, each one shell token, with a trailing space
    cy.get("@insertInput-terminal-a").should((subject) => {
      const stub = subject as unknown as sinon.SinonStub;
      expect(stub.callCount, "one run of paths").to.equal(1);
      expect(stub.firstCall.args[0]).to.equal(
        `'${A_SCREENSHOT}' '/Users/me/it'\\''s ✓ – notes.txt' `,
      );
    });

    // ... the highlight clears, and nothing was read or uploaded — the agent opens the files itself
    page.dropOverlay({ timeout: 2000 }).should("not.exist");
    cy.wrap(null).should(() => {
      expect(drop.reads, "no bytes read").to.have.length(0);
      expect(backend.callsTo(SessionFilesService.method.uploadSessionFileChunk)).to.have.length(0);
    });
  });

  it("uploads the dropped files into a session that cannot read this machine's files", () => {
    // Given a session on another host (or confined on this one)
    const drop = aDropOf({
      [A_SCREENSHOT]: SCREENSHOT_BYTES,
      [A_QUOTED_NAME]: NOTES_BYTES,
    });
    const backend = anUploadBackend();
    const terminal = aTerminal("terminal-a", false);
    mountTerminals(drop, backend, [terminal]);

    // When two files are dropped on its terminal
    dropOn(drop, "terminal-a", [A_SCREENSHOT, A_QUOTED_NAME]);

    // Then the uploaded copies' paths are typed, in drop order
    cy.get("@insertInput-terminal-a").should((subject) => {
      const stub = subject as unknown as sinon.SinonStub;
      expect(stub.callCount, "one run of paths").to.equal(1);
      expect(stub.firstCall.args[0]).to.equal(
        `'${uploadedPathOf("Screen Shot 2026-10-25 at 10.00.00.png")}' ` +
          `'${uploadedPathOf("it'\\''s ✓ – notes.txt")}' `,
      );
    });

    // ... and what reached the session's host is exactly the bytes on this machine
    cy.wrap(null).should(() => {
      expect(uploadedBytes(backend, "Screen Shot 2026-10-25 at 10.00.00.png")).to.deep.equal(
        Array.from(SCREENSHOT_BYTES),
      );
      expect(uploadedBytes(backend, "it's ✓ – notes.txt")).to.deep.equal(Array.from(NOTES_BYTES));
    });
  });

  it("refuses a dropped folder with a visible message, and still uploads the files beside it", () => {
    // Given a session that cannot read this machine's files
    const drop = aDropOf({
      [A_FOLDER]: "folder",
      [A_SCREENSHOT]: SCREENSHOT_BYTES,
    });
    const backend = anUploadBackend();
    const terminal = aTerminal("terminal-a", false);
    mountTerminals(drop, backend, [terminal]);

    // When a folder and a file are dropped together
    dropOn(drop, "terminal-a", [A_FOLDER, A_SCREENSHOT]);

    // Then the folder is refused, by name, where the operator is looking
    page.dropRefusal().should("be.visible").and("contain.text", "a folder");

    // ... and only the file is uploaded and typed
    cy.get("@insertInput-terminal-a").should((subject) => {
      const stub = subject as unknown as sinon.SinonStub;
      expect(stub.callCount).to.equal(1);
      expect(stub.firstCall.args[0]).to.equal(
        `'${uploadedPathOf("Screen Shot 2026-10-25 at 10.00.00.png")}' `,
      );
    });
    cy.wrap(null).should(() => {
      expect(drop.reads.map((read) => read.path)).not.to.include(A_FOLDER);
    });
  });

  it("delivers a drop to the terminal under the pointer when several are open", () => {
    // Given two session terminals side by side
    const drop = aDropOf({ [A_SCREENSHOT]: SCREENSHOT_BYTES });
    const first = aTerminal("terminal-a", true);
    const second = aTerminal("terminal-b", true);
    mountTerminals(drop, anUploadBackend(), [first, second]);

    // When a file is dropped on the second
    dropOn(drop, "terminal-b", [A_SCREENSHOT]);

    // Then only the second receives it
    cy.get("@insertInput-terminal-b").should("have.been.calledOnceWith", `'${A_SCREENSHOT}' `);
    cy.get("@insertInput-terminal-a").should("not.have.been.called");
  });

  it("ignores a drop that lands outside every terminal", () => {
    // Given
    const drop = aDropOf({ [A_SCREENSHOT]: SCREENSHOT_BYTES });
    const backend = anUploadBackend();
    const terminal = aTerminal("terminal-a", false);
    mountTerminals(drop, backend, [terminal]);

    // When a file is dropped beside the terminal
    dropOn(drop, "not-a-terminal", [A_SCREENSHOT]);

    // Then nothing is typed, read or uploaded
    cy.wait(300);
    cy.get("@insertInput-terminal-a").should("not.have.been.called");
    cy.wrap(null).should(() => {
      expect(drop.described).to.have.length(0);
      expect(backend.callsTo(SessionFilesService.method.uploadSessionFileChunk)).to.have.length(0);
    });
  });
});
