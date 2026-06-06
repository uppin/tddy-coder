/**
 * Cypress component tests for GhosttyTerminalGrpc.
 *
 * GhosttyTerminalGrpc wraps GhosttyTerminal and connects terminal I/O to the gRPC
 * StreamSessionTerminalIO bidi stream (ConnectionService) instead of a LiveKit DataChannel.
 *
 * PRD: docs/ft/daemon/claude-cli-session.md
 *
 * ALL TESTS CURRENTLY FAIL:
 * - GhosttyTerminalGrpc does not yet exist → import error on every test.
 */

import React from "react";
// FAILS: module does not exist yet — implement at packages/tddy-web/src/components/GhosttyTerminalGrpc.tsx
import { GhosttyTerminalGrpc } from "../../src/components/GhosttyTerminalGrpc";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** A minimal fake gRPC bidi stream matching the StreamSessionTerminalIO shape. */
function makeFakeGrpcStream() {
  const sentChunks: Uint8Array[] = [];
  const outputListeners: Array<(data: Uint8Array) => void> = [];

  return {
    /** Simulate the server pushing bytes to the terminal. */
    pushOutput(data: Uint8Array) {
      outputListeners.forEach((fn) => fn(data));
    },
    /** The stream object passed as a prop to GhosttyTerminalGrpc. */
    stream: {
      send(data: Uint8Array) {
        sentChunks.push(data);
      },
      onMessage(fn: (data: Uint8Array) => void) {
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
    // FAILS: GhosttyTerminalGrpc does not exist.
    const fake = makeFakeGrpcStream();
    cy.mount(
      <div style={{ height: 400, width: 800, position: "relative" }}>
        <GhosttyTerminalGrpc
          sessionToken="fake-token"
          sessionId="01900000-0000-7000-8000-000000000001"
          stream={fake.stream}
        />
      </div>
    );
    cy.get("[data-testid='ghostty-terminal']", { timeout: 10000 }).should("exist");
  });

  it("displays output bytes received from the gRPC stream", () => {
    // FAILS: GhosttyTerminalGrpc does not exist.
    const fake = makeFakeGrpcStream();
    cy.mount(
      <div style={{ height: 400, width: 800, position: "relative" }}>
        <GhosttyTerminalGrpc
          sessionToken="fake-token"
          sessionId="01900000-0000-7000-8000-000000000001"
          stream={fake.stream}
        />
      </div>
    );
    cy.get("[data-testid='ghostty-terminal']", { timeout: 10000 }).should("exist");

    // Simulate the server sending "Hello world\r\n" to the terminal.
    const payload = new TextEncoder().encode("Hello world\r\n");
    cy.then(() => {
      fake.pushOutput(payload);
    });

    // The terminal canvas should render the text.
    cy.get("[data-testid='ghostty-terminal']").should("contain.text", "Hello world");
  });

  it("forwards keyboard input as bytes to the gRPC stream", () => {
    // FAILS: GhosttyTerminalGrpc does not exist.
    const fake = makeFakeGrpcStream();
    cy.mount(
      <div style={{ height: 400, width: 800, position: "relative" }}>
        <GhosttyTerminalGrpc
          sessionToken="fake-token"
          sessionId="01900000-0000-7000-8000-000000000001"
          stream={fake.stream}
        />
      </div>
    );
    cy.get("[data-testid='ghostty-terminal']", { timeout: 10000 })
      .should("exist")
      .focus()
      .type("ls");

    cy.then(() => {
      // Input bytes for "ls" must have been forwarded to the stream.
      const allSent = fake.sentChunks
        .map((c) => new TextDecoder().decode(c))
        .join("");
      expect(allSent).to.include("l");
      expect(allSent).to.include("s");
    });
  });

  it("sends OSC resize sequence when the container is resized", () => {
    // FAILS: GhosttyTerminalGrpc does not exist.
    const fake = makeFakeGrpcStream();
    cy.mount(
      <div
        id="resize-wrapper"
        style={{ height: 400, width: 800, position: "relative" }}
      >
        <GhosttyTerminalGrpc
          sessionToken="fake-token"
          sessionId="01900000-0000-7000-8000-000000000002"
          stream={fake.stream}
        />
      </div>
    );
    cy.get("[data-testid='ghostty-terminal']", { timeout: 10000 }).should("exist");

    // Trigger a resize by changing the container dimensions.
    cy.get("#resize-wrapper").invoke("css", "width", "600px").invoke("css", "height", "300px");

    cy.then(() => {
      // At least one resize OSC sequence must have been sent:  \x1b]resize;{cols};{rows}\x07
      const allSent = fake.sentChunks
        .map((c) => new TextDecoder().decode(c))
        .join("");
      expect(allSent).to.match(/\x1b\]resize;\d+;\d+\x07/);
    });
  });

  it("shows a connection status dot", () => {
    // FAILS: GhosttyTerminalGrpc does not exist.
    const fake = makeFakeGrpcStream();
    cy.mount(
      <div style={{ height: 400, width: 800, position: "relative" }}>
        <GhosttyTerminalGrpc
          sessionToken="fake-token"
          sessionId="01900000-0000-7000-8000-000000000003"
          stream={fake.stream}
          connectionOverlay
        />
      </div>
    );
    cy.get("[data-testid='connection-status-dot']", { timeout: 10000 }).should("exist");
  });

  it("calls onDisconnect when the Disconnect menu item is clicked", () => {
    // FAILS: GhosttyTerminalGrpc does not exist.
    const fake = makeFakeGrpcStream();
    const onDisconnect = cy.stub().as("onDisconnect");
    cy.mount(
      <div style={{ height: 400, width: 800, position: "relative" }}>
        <GhosttyTerminalGrpc
          sessionToken="fake-token"
          sessionId="01900000-0000-7000-8000-000000000004"
          stream={fake.stream}
          connectionOverlay
          onDisconnect={onDisconnect}
        />
      </div>
    );
    cy.get("[data-testid='connection-status-dot']", { timeout: 10000 }).click();
    cy.get("[data-testid='connection-menu-disconnect']").click({ force: true });
    cy.get("@onDisconnect").should("have.been.calledOnce");
  });
});
