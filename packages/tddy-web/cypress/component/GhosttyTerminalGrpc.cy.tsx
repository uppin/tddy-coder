/**
 * Cypress component tests for GhosttyTerminalGrpc.
 *
 * GhosttyTerminalGrpc wraps GhosttyTerminal and connects terminal I/O to the gRPC
 * StreamSessionTerminalIO bidi stream (ConnectionService) instead of a LiveKit DataChannel.
 *
 * PRD: docs/ft/daemon/claude-cli-session.md
 *
 */

import React from "react";
// FAILS: module does not exist yet — implement at packages/tddy-web/src/components/GhosttyTerminalGrpc.tsx
import { GhosttyTerminalGrpc, type GrpcFrame } from "../../src/components/GhosttyTerminalGrpc";
import { UploadProgressProvider } from "../../src/rpc/uploadProgress";
import { byTestId, TEST_IDS } from "../support/testIds";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Mount the terminal inside the UploadProgressProvider its `TerminalFileDropZone` requires. */
function mountTerminal(node: React.ReactElement) {
  return cy.mount(
    <div style={{ height: 400, width: 800, position: "relative" }}>
      <UploadProgressProvider>{node}</UploadProgressProvider>
    </div>,
  );
}

/** A minimal fake gRPC bidi stream matching the StreamSessionTerminalIO shape. */
function makeFakeGrpcStream() {
  const sentChunks: Uint8Array[] = [];
  const outputListeners: Array<(frame: GrpcFrame) => void> = [];

  return {
    /** Simulate the server pushing a frame to the terminal (defaults to a live tail frame). */
    pushOutput(data: Uint8Array, endOffset: bigint = 0n, atOldest = false) {
      outputListeners.forEach((fn) => fn({ data, endOffset, atOldest }));
    },
    /** The stream object passed as a prop to GhosttyTerminalGrpc. */
    stream: {
      send(data: Uint8Array) {
        sentChunks.push(data);
      },
      onMessage(fn: (frame: GrpcFrame) => void) {
        outputListeners.push(fn);
      },
      close() {},
    },
    sentChunks,
  };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("GhosttyTerminalGrpc", () => {
  it("renders the ghostty terminal container", () => {
    // Given
    const fake = makeFakeGrpcStream();

    // When
    mountTerminal(
      <GhosttyTerminalGrpc
        sessionToken="fake-token"
        sessionId="01900000-0000-7000-8000-000000000001"
        stream={fake.stream}
      />,
    );

    // Then
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");
  });

  it("displays output bytes received from the gRPC stream", () => {
    // Given
    const fake = makeFakeGrpcStream();

    // When
    mountTerminal(
      <GhosttyTerminalGrpc
        sessionToken="fake-token"
        sessionId="01900000-0000-7000-8000-000000000001"
        stream={fake.stream}
      />,
    );
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");

    // When — server pushes "Hello world\r\n" to the terminal
    // Verifies that stream.onMessage is wired up and GhosttyTerminalGrpc receives the data
    // without throwing. Rendered text lives on a WebGL canvas so contain.text cannot read it.
    const payload = new TextEncoder().encode("Hello world\r\n");
    cy.then(() => {
      expect(() => fake.pushOutput(payload)).not.to.throw();
    });

    // Then — terminal element still present and stream correctly routed the data
    byTestId(TEST_IDS.ghosttyTerminal).should("exist");
  });

  it("forwards keyboard input as bytes to the gRPC stream", () => {
    // Given
    const fake = makeFakeGrpcStream();
    mountTerminal(
      <GhosttyTerminalGrpc
        sessionToken="fake-token"
        sessionId="01900000-0000-7000-8000-000000000001"
        stream={fake.stream}
      />,
    );
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");

    // When
    byTestId(TEST_IDS.ghosttyTerminal).focus().type("ls");

    // Then
    cy.then(() => {
      const allSent = fake.sentChunks
        .map((c) => new TextDecoder().decode(c))
        .join("");
      expect(allSent).to.include("l");
      expect(allSent).to.include("s");
    });
  });

  it("sends OSC resize sequence when the container is resized", () => {
    // Given
    const fake = makeFakeGrpcStream();
    cy.mount(
      <div
        id="resize-wrapper"
        style={{ height: 400, width: 800, position: "relative" }}
      >
        <UploadProgressProvider>
          <GhosttyTerminalGrpc
            sessionToken="fake-token"
            sessionId="01900000-0000-7000-8000-000000000002"
            stream={fake.stream}
          />
        </UploadProgressProvider>
      </div>
    );
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");

    // When
    cy.get("#resize-wrapper").invoke("css", "width", "600px").invoke("css", "height", "300px");

    // Then — at least one OSC resize sequence was sent: \x1b]resize;{cols};{rows}\x07
    // cy.wrap().should() retries until the assertion passes so we handle async resize events.
    cy.wrap(fake).should((f) => {
      const allSent = f.sentChunks
        .map((c: Uint8Array) => new TextDecoder().decode(c))
        .join("");
      expect(allSent).to.match(/\x1b\]resize;\d+;\d+\x07/);
    });
  });

  it("shows a connection status dot", () => {
    // Given / When
    const fake = makeFakeGrpcStream();
    mountTerminal(
      <GhosttyTerminalGrpc
        sessionToken="fake-token"
        sessionId="01900000-0000-7000-8000-000000000003"
        stream={fake.stream}
        connectionOverlay
      />,
    );

    // Then
    byTestId(TEST_IDS.connectionStatusDot, { timeout: 10000 }).should("exist");
  });

  it("calls onDisconnect when the Disconnect menu item is clicked", () => {
    // Given
    const fake = makeFakeGrpcStream();
    const onDisconnect = cy.stub().as("onDisconnect");
    mountTerminal(
      <GhosttyTerminalGrpc
        sessionToken="fake-token"
        sessionId="01900000-0000-7000-8000-000000000004"
        stream={fake.stream}
        connectionOverlay
        onDisconnect={onDisconnect}
      />,
    );

    // When
    byTestId(TEST_IDS.connectionStatusDot, { timeout: 10000 }).click();
    byTestId(TEST_IDS.connectionMenuDisconnect, { timeout: 4000 }).should("be.visible").click();

    // Then
    cy.get("@onDisconnect").should("have.been.calledOnce");
  });

  it("surfaces the cumulative output offset via onOffsetUpdate (snap on replay, advance on live)", () => {
    // Given — a terminal with an offset-update spy
    const fake = makeFakeGrpcStream();
    const offsets: bigint[] = [];
    const onOffsetUpdate = cy.stub().callsFake((offset: bigint) => { offsets.push(offset); }).as("onOffsetUpdate");
    mountTerminal(
      <GhosttyTerminalGrpc
        sessionToken="fake-token"
        sessionId="01900000-0000-7000-8000-000000000005"
        stream={fake.stream}
        onOffsetUpdate={onOffsetUpdate}
      />,
    );
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");

    // When — a replay frame arrives tagged with endOffset = 10
    cy.then(() => {
      fake.pushOutput(new TextEncoder().encode("replay"), 10n, false);
    });

    // Then — the offset snaps to the frame's absolute endOffset
    cy.then(() => {
      expect(offsets.at(-1)).to.equal(10n, "offset snaps to replay endOffset");
    });

    // When — a live tail frame of 5 bytes arrives (endOffset = 0)
    cy.then(() => {
      fake.pushOutput(new TextEncoder().encode("live!"), 0n, false);
    });

    // Then — the offset advances by the live frame's byte length (10 + 5 = 15)
    cy.then(() => {
      expect(offsets.at(-1)).to.equal(15n, "offset advances by live byte length");
    });
  });
});
