/**
 * Acceptance tests — input-offset accounting and the enqueued-input overlay's ACK behavior.
 *
 * docs/ft/web/enqueued-input-overlay.md
 *
 * Two behaviors are pinned here:
 *
 *  A. Wire contract — every `SendTerminalInput` the terminal emits carries a cumulative byte
 *     offset: for consecutive calls `offset[i] == offset[i-1] + data[i].length`, and the first
 *     call's offset equals its own byte length. Exercised through the real `GrpcSessionTerminal`
 *     against an in-memory ConnectionService backend.
 *
 *  B. Overlay timing / collapse — through the real `useEnqueuedInput` hook: the overlay appears
 *     only after 500ms without an ACK, collapses from the front as prefix offsets are ACKed
 *     ("HELLO WORLD" + ack 3 -> "LO WORLD"), hides once fully ACKed, and never appears when the
 *     ACK beats the threshold.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  ConnectionService,
  SendTerminalInputResponseSchema,
} from "../../src/gen/connection_pb";
import { GrpcSessionTerminal } from "../../src/components/sessions/GrpcSessionTerminal";
import { useEnqueuedInput } from "../../src/components/sessions/useEnqueuedInput";
import { EnqueuedInputOverlay } from "../../src/components/connection/EnqueuedInputOverlay";
import { UploadProgressProvider } from "../../src/rpc/uploadProgress";
import { byTestId, TEST_IDS } from "../support/testIds";

// ---------------------------------------------------------------------------
// A. Wire contract — cumulative byte offset on SendTerminalInput
// ---------------------------------------------------------------------------

/** Backend that records SendTerminalInput and keeps the output stream open (no ACKs). */
function aRecordingBackend() {
  return anInMemoryRpcBackend()
    .onUnary(ConnectionService.method.sendTerminalInput, () =>
      create(SendTerminalInputResponseSchema, {}),
    )
    .implement(ConnectionService, {
      // Long-lived, silent output stream — Part A asserts only on what the client sends.
      // eslint-disable-next-line require-yield
      async *streamTerminalOutput() {
        await new Promise<never>(() => {});
      },
    });
}

describe("Terminal input offset — wire contract", () => {
  it("sends a cumulative byte offset on each SendTerminalInput", () => {
    // Given — a connected terminal whose input flows over the in-memory backend
    const backend = aRecordingBackend();
    const client = createClient(ConnectionService, backend.transport());

    cy.mount(
      <UploadProgressProvider>
        <div style={{ height: 400, width: 800, position: "relative" }}>
          <GrpcSessionTerminal
            sessionId="01900000-0000-7000-8000-0000000000aa"
            sessionToken="fake-token"
            client={client}
            connected={{ sessionId: "01900000-0000-7000-8000-0000000000aa", controlToken: "lease-1" }}
          />
        </div>
      </UploadProgressProvider>,
    );
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");

    // When — the user types into the real terminal
    byTestId(TEST_IDS.ghosttyTerminal).focus().type("abc");

    // Then — offsets are the running byte total of everything sent (keystrokes + any resize OSC)
    cy.wrap(backend)
      .should((b) => {
        const calls = b.callsTo(ConnectionService.method.sendTerminalInput);
        // wait until the three keystrokes have been recorded
        expect(calls.length).to.be.greaterThan(2);
      })
      .then((b: ReturnType<typeof aRecordingBackend>) => {
        const calls = b.callsTo(ConnectionService.method.sendTerminalInput);
        expect(Number(calls[0].inputOffset)).to.equal(calls[0].data.length);
        for (let i = 1; i < calls.length; i++) {
          expect(Number(calls[i].inputOffset)).to.equal(
            Number(calls[i - 1].inputOffset) + calls[i].data.length,
          );
        }
      });
  });
});

// ---------------------------------------------------------------------------
// B. Overlay timing / collapse — driven through the real useEnqueuedInput hook
// ---------------------------------------------------------------------------

const OVERLAY_DELAY_MS = 500;

/** Harness that drives the hook deterministically — no real terminal, no RPC. */
function EnqueuedInputHarness() {
  const { enqueue, ack, model, visible } = useEnqueuedInput({
    delayMs: OVERLAY_DELAY_MS,
    maxItems: 40,
  });

  const typeHelloWorld = () => {
    for (const ch of "HELLO WORLD") {
      enqueue(new TextEncoder().encode(ch));
    }
  };

  return (
    <div>
      <button data-testid="hx-type" onClick={typeHelloWorld}>
        type
      </button>
      <button data-testid="hx-ack-3" onClick={() => ack(3)}>
        ack 3
      </button>
      <button data-testid="hx-ack-11" onClick={() => ack(11)}>
        ack 11
      </button>
      <EnqueuedInputOverlay model={model} visible={visible} />
    </div>
  );
}

describe("Enqueued-input overlay — ACK behavior", () => {
  it("shows the overlay only after 500ms with no ACK", () => {
    cy.clock();
    cy.mount(<EnqueuedInputHarness />);

    // When — type, then let less than the threshold pass
    byTestId("hx-type").click();
    cy.tick(OVERLAY_DELAY_MS - 1);
    byTestId(TEST_IDS.enqueuedInputOverlay).should("not.exist");

    // Then — crossing 500ms reveals the overlay with the un-acked text
    cy.tick(1);
    byTestId(TEST_IDS.enqueuedInputOverlay).should("exist");
    byTestId(TEST_IDS.enqueuedInputText).should("have.text", "HELLO WORLD");
  });

  it("collapses the overlay to the un-acked suffix when the server ACKs a prefix offset", () => {
    cy.clock();
    cy.mount(<EnqueuedInputHarness />);

    byTestId("hx-type").click();
    cy.tick(OVERLAY_DELAY_MS);
    byTestId(TEST_IDS.enqueuedInputText).should("have.text", "HELLO WORLD");

    // When — the server acks the first 3 bytes ("HEL")
    byTestId("hx-ack-3").click();

    // Then — the overlay collapses to what remains un-acked
    byTestId(TEST_IDS.enqueuedInputText).should("have.text", "LO WORLD");
  });

  it("hides the overlay once every byte is ACKed", () => {
    cy.clock();
    cy.mount(<EnqueuedInputHarness />);

    byTestId("hx-type").click();
    cy.tick(OVERLAY_DELAY_MS);
    byTestId(TEST_IDS.enqueuedInputOverlay).should("exist");

    // When — the server acks all 11 bytes
    byTestId("hx-ack-11").click();

    // Then
    byTestId(TEST_IDS.enqueuedInputOverlay).should("not.exist");
  });

  it("never shows the overlay when the ACK beats the 500ms threshold", () => {
    cy.clock();
    cy.mount(<EnqueuedInputHarness />);

    // When — the ack lands before the threshold elapses
    byTestId("hx-type").click();
    byTestId("hx-ack-11").click();
    cy.tick(OVERLAY_DELAY_MS);

    // Then — the overlay was never mounted
    byTestId(TEST_IDS.enqueuedInputOverlay).should("not.exist");
  });
});
