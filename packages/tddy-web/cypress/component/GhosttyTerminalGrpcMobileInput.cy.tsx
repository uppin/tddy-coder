/**
 * Behaviour spec: the Sessions-screen terminal (GhosttyTerminalGrpc) must accept
 * text typed on a mobile soft keyboard and forward it to the PTY stream.
 *
 * Soft keyboards fire DOM `input` events (not `keydown` with a `key`), so
 * ghostty-web's `onData` never sees them. The terminal therefore needs the same
 * hidden-input mobile keyboard affordance that GhosttyTerminalLiveKit has.
 *
 * Fails today: GhosttyTerminalGrpc renders no mobile keyboard affordance, so the
 * typed character never reaches `stream.send`.
 */

import React from "react";
import { GhosttyTerminalGrpc, type GrpcStream } from "../../src/components/GhosttyTerminalGrpc";
import { byTestId, TEST_IDS, shortcutButton } from "../support/testIds";

const TYPED_CHAR = "x";
const TYPED_CHAR_BYTE = 0x78; // UTF-8 for "x"

const SHIFT_TAB_SHORTCUT = { label: "Shift+Tab", keys: ["Shift", "Tab"] };
const SHIFT_TAB_BYTES = [0x1b, 0x5b, 0x5a]; // CSI Z (reverse tab)

function aCapturingStream(): GrpcStream {
  return {
    send: cy.stub().as("streamSend"),
    onMessage: () => {},
    close: () => {},
  };
}

function mountSessionsTerminalOnMobile(
  stream: GrpcStream,
  mobileShortcuts?: { label: string; keys: string[] }[],
) {
  cy.viewport(375, 667);
  cy.mount(
    <div style={{ width: 375, height: 500, position: "relative" }}>
      <GhosttyTerminalGrpc
        sessionToken="test-token"
        sessionId="test-session"
        stream={stream}
        mobileShortcuts={mobileShortcuts}
      />
    </div>,
  );
}

function typeOnMobileKeyboard(char: string) {
  byTestId(TEST_IDS.mobileKeyboardButton).within(() => {
    cy.get("input").focus().type(char);
  });
}

function expectStreamReceivedByte(byte: number) {
  cy.get("@streamSend").should((subject) => {
    const stub = subject as unknown as { getCalls: () => { args: unknown[] }[] };
    const received = stub.getCalls().some((call) => {
      const arg = call.args[0];
      return arg instanceof Uint8Array && arg.length === 1 && arg[0] === byte;
    });
    expect(received, `stream.send should receive the typed character byte 0x${byte.toString(16)}`).to.be.true;
  });
}

function expectStreamReceivedBytes(bytes: number[]) {
  cy.get("@streamSend").should((subject) => {
    const stub = subject as unknown as { getCalls: () => { args: unknown[] }[] };
    const received = stub.getCalls().some((call) => {
      const arg = call.args[0];
      return (
        arg instanceof Uint8Array &&
        arg.length === bytes.length &&
        bytes.every((b, i) => arg[i] === b)
      );
    });
    expect(received, `stream.send should receive bytes [${bytes.join(", ")}]`).to.be.true;
  });
}

describe("Sessions-screen terminal — mobile soft-keyboard input", () => {
  it("forwards a character typed on the mobile keyboard to the PTY stream", () => {
    // Given — a connected gRPC terminal on a mobile viewport
    const stream = aCapturingStream();
    mountSessionsTerminalOnMobile(stream);

    // Then — a mobile keyboard affordance is available
    byTestId(TEST_IDS.mobileKeyboardButton).should("exist");

    // When — the user types a character on the soft keyboard
    typeOnMobileKeyboard(TYPED_CHAR);

    // Then — the character reaches the PTY stream as its UTF-8 byte
    expectStreamReceivedByte(TYPED_CHAR_BYTE);
  });
});

describe("Sessions-screen terminal — mobile shortcut overlay", () => {
  it("shows the shortcut overlay on a mobile viewport when shortcuts are provided", () => {
    // Given — a connected gRPC terminal on mobile with a shortcut preset
    const stream = aCapturingStream();
    mountSessionsTerminalOnMobile(stream, [SHIFT_TAB_SHORTCUT]);

    // Then — the shortcut overlay is rendered
    byTestId(TEST_IDS.shortcutDrawer).should("exist");
  });

  it("sends the shortcut sequence to the PTY stream when a shortcut is tapped", () => {
    // Given — a connected gRPC terminal on mobile with a Shift+Tab shortcut
    const stream = aCapturingStream();
    mountSessionsTerminalOnMobile(stream, [SHIFT_TAB_SHORTCUT]);
    byTestId(TEST_IDS.shortcutDrawer).should("exist");

    // Given — the overlay is collapsed by default; tap the control to expand it
    byTestId(TEST_IDS.shortcutDragHandle)
      .trigger("pointerdown", { clientX: 200, clientY: 600, pointerId: 1, force: true })
      .trigger("pointerup", { clientX: 200, clientY: 600, pointerId: 1, force: true });

    // When — the user taps the Shift+Tab shortcut
    byTestId(shortcutButton(SHIFT_TAB_SHORTCUT.label)).click({ force: true });

    // Then — the reverse-tab sequence reaches the PTY stream
    expectStreamReceivedBytes(SHIFT_TAB_BYTES);
  });
});
