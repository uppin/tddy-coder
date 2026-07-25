/**
 * Behaviour spec: while files upload, an aggregate progress bar is visible in
 * the screen-level bottom strip (Host Stats Footer), and it auto-hides once the
 * upload completes.
 *
 * Fails today: there is no `UploadProgressIndicator` in the footer and no
 * upload orchestration to drive it.
 *
 * PRD: docs/ft/web/host-stats-footer.md § Upload progress
 */

import React from "react";
import { HostStatsFooter } from "../../src/components/sessions/HostStatsFooter";
import { TerminalFileDropZone } from "../../src/components/connection/TerminalFileDropZone";
import { UploadProgressProvider } from "../../src/rpc/uploadProgress";
import { ConnectionService } from "../../src/gen/connection_pb";
import { aConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { terminalFileUploadPage as page } from "../support/pages/terminalFileUploadPage";
import { hostStatsFooterPage as footer } from "../support/pages/hostStatsFooterPage";
import { aFile, dropFilesOnto } from "../support/util/fileDrop";

const HOST_DISK = { availableBytes: 42_100_000_000n, totalBytes: 100_000_000_000n, projectDir: "/home/tddy/repos" };

/**
 * Mount the footer next to a drop zone, sharing one `UploadProgressProvider`. `gate` (if given)
 * holds every upload chunk until resolved, so a test can observe the mid-upload state.
 */
function mountFooterWithDropZone(gate?: Promise<void>) {
  const backend = aConnectionServiceBackend({ hostCpuPerCore: [12], hostDisk: HOST_DISK }).onUnary(
    ConnectionService.method.uploadSessionFileChunk,
    async (req) => {
      if (gate) await gate;
      return { hostPath: req.last ? `/srv/host/uploads/${req.fileName}` : "" };
    },
  );
  mountWithRpc(
    withSelectedDaemon(
      <UploadProgressProvider>
        <TerminalFileDropZone sessionToken="tok" sessionId="s1" insertInput={cy.stub()}>
          <div data-testid="ghostty-terminal" style={{ width: 400, height: 300 }} />
        </TerminalFileDropZone>
        <HostStatsFooter attachment={{ status: "idle" }} />
      </UploadProgressProvider>,
    ),
    backend,
  );
}

describe("Terminal file upload — progress in the bottom strip", () => {
  it("shows an aggregate progress bar inside the footer while files upload", () => {
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    mountFooterWithDropZone(gate);

    // When — two files are dropped and their upload is held mid-flight
    dropFilesOnto(page.dropZoneSelector, [aFile("a.txt", "AAAA"), aFile("b.txt", "BBBB")]);

    // Then — the progress indicator appears inside the bottom strip, reporting both files
    footer.footer().should("exist");
    page.progressIndicatorInFooter().should("exist");
    page.progressIndicator().should("have.attr", "data-upload-file-count", "2");
    // Every chunk is held at the gate, so no bytes have completed — percent is deterministically 0.
    page.progressIndicator().should("have.attr", "data-upload-percent", "0");

    // Release the held upload so the test does not leak a pending promise
    cy.then(() => release());
  });

  it("auto-hides the progress bar after the upload completes", () => {
    mountFooterWithDropZone();

    // When — a file is dropped and its upload runs to completion
    dropFilesOnto(page.dropZoneSelector, [aFile("done.txt", "x")]);

    // Then — the indicator eventually disappears on its own
    page.progressIndicator().should("not.exist");
  });
});
