/**
 * Cypress component tests for `GhosttyTerminalSession`.
 *
 * The one terminal wraps `GhosttyTerminal` and renders whatever arrives on the feed its caller
 * opened — a daemon's `StreamTerminalOutput`, a room's `StreamTerminalIO`, or a double.
 *
 * PRD: docs/ft/daemon/claude-cli-session.md
 *
 */

import React from "react";
import { GhosttyTerminalSession } from "../../src/components/GhosttyTerminalSession";
import type { TerminalFrame } from "../../src/rpc/connections/terminal";
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

/** A minimal feed double: bytes the test pushes in, and the bytes the terminal writes back. */
function aTerminalFeedDouble() {
  const sentChunks: Uint8Array[] = [];
  const outputListeners: Array<(frame: TerminalFrame) => void> = [];

  return {
    /** Simulate the server pushing a frame to the terminal (defaults to a live tail frame). */
    pushOutput(data: Uint8Array, endOffset: bigint = 0n, atOldest = false) {
      outputListeners.forEach((fn) => fn({ data, endOffset, atOldest }));
    },
    /** The stream half of the feed passed to the terminal. */
    stream: {
      send(data: Uint8Array) {
        sentChunks.push(data);
      },
      onMessage(fn: (frame: TerminalFrame) => void) {
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

describe("GhosttyTerminalSession", () => {
  it("renders the ghostty terminal container", () => {
    // Given
    const fake = aTerminalFeedDouble();

    // When
    mountTerminal(
      <GhosttyTerminalSession
        sessionToken="fake-token"
        sessionId="01900000-0000-7000-8000-000000000001"
        feed={{ stream: fake.stream }}
      />,
    );

    // Then
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");
  });

  it("paints the output bytes that arrive on the feed", () => {
    // Given a mounted terminal
    const fake = aTerminalFeedDouble();
    mountTerminal(
      <GhosttyTerminalSession
        sessionToken="fake-token"
        sessionId="01900000-0000-7000-8000-000000000001"
        feed={{ stream: fake.stream }}
      />,
    );
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");

    // When the far end writes
    cy.then(() => fake.pushOutput(new TextEncoder().encode("Hello world\r\n")));

    // Then the bytes reached the emulator's buffer. The canvas is WebGL, so the assertion reads
    // the hidden mirror of what ghostty painted — a terminal that drops every byte fails here.
    byTestId(TEST_IDS.terminalBufferText, { timeout: 10000 }).should("contain.text", "Hello world");
  });

  it("forwards keyboard input as bytes to the terminal stream", () => {
    // Given
    const fake = aTerminalFeedDouble();
    mountTerminal(
      <GhosttyTerminalSession
        sessionToken="fake-token"
        sessionId="01900000-0000-7000-8000-000000000001"
        feed={{ stream: fake.stream }}
      />,
    );
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");

    // When
    byTestId(TEST_IDS.ghosttyTerminal).focus().type("ls");

    // Then — each keystroke arrives as its own chunk. Asserted chunk by chunk rather than over the
    // concatenation, which also holds the resize OSC (`\x1b]resize;80;24\x07`) and so contains an
    // "s" whether or not anything was typed.
    cy.wrap(null, { timeout: 4000 }).should(() => {
      const chunks = fake.sentChunks.map((c) => new TextDecoder().decode(c));
      expect(chunks, "chunks written back to the feed").to.include("l");
      expect(chunks, "chunks written back to the feed").to.include("s");
    });
  });

  it("sends OSC resize sequence when the container is resized", () => {
    // Given
    const fake = aTerminalFeedDouble();
    cy.mount(
      <div
        id="resize-wrapper"
        style={{ height: 400, width: 800, position: "relative" }}
      >
        <UploadProgressProvider>
          <GhosttyTerminalSession
            sessionToken="fake-token"
            sessionId="01900000-0000-7000-8000-000000000002"
            feed={{ stream: fake.stream }}
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
    const fake = aTerminalFeedDouble();
    mountTerminal(
      <GhosttyTerminalSession
        sessionToken="fake-token"
        sessionId="01900000-0000-7000-8000-000000000003"
        feed={{ stream: fake.stream }}
        connectionOverlay={{ onDisconnect: () => {} }}
      />,
    );

    // Then
    byTestId(TEST_IDS.connectionStatusDot, { timeout: 10000 }).should("exist");
  });

  it("calls onDisconnect when the Disconnect menu item is clicked", () => {
    // Given
    const fake = aTerminalFeedDouble();
    const onDisconnect = cy.stub().as("onDisconnect");
    mountTerminal(
      <GhosttyTerminalSession
        sessionToken="fake-token"
        sessionId="01900000-0000-7000-8000-000000000004"
        feed={{ stream: fake.stream }}
        connectionOverlay={{ onDisconnect }}
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
    const fake = aTerminalFeedDouble();
    const offsets: bigint[] = [];
    const onOffsetUpdate = cy.stub().callsFake((offset: bigint) => { offsets.push(offset); }).as("onOffsetUpdate");
    mountTerminal(
      <GhosttyTerminalSession
        sessionToken="fake-token"
        sessionId="01900000-0000-7000-8000-000000000005"
        feed={{ stream: fake.stream }}
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
