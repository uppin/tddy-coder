/**
 * Acceptance tests — enqueued-input overlay presentation.
 *
 * The overlay is a single line reflecting terminal input that the server has not yet
 * acknowledged (docs/ft/web/enqueued-input-overlay.md). These tests pin the presentation
 * contract of `EnqueuedInputOverlay` against explicit overlay models:
 *   - un-acked keyboard text renders inline on one line,
 *   - a run of mouse events collapses to a single glyph carrying the event count,
 *   - items past the single-line budget collapse into a trailing overflow glyph + count,
 *   - nothing renders when the overlay is not visible.
 */

import React from "react";
import {
  EnqueuedInputOverlay,
} from "../../src/components/connection/EnqueuedInputOverlay";
import type {
  OverlayModel,
  OverlaySegment,
} from "../../src/lib/terminalInputQueue";
import { byTestId, TEST_IDS } from "../support/testIds";

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

function anOverlayModel(segments: OverlaySegment[], overflowCount = 0): OverlayModel {
  return { segments, overflowCount };
}

const text = (value: string): OverlaySegment => ({ kind: "text", text: value });
const mouseRun = (count: number): OverlaySegment => ({ kind: "mouse", count });

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

function anOverlay(model: OverlayModel, visible = true) {
  const driver = {
    mount() {
      cy.mount(<EnqueuedInputOverlay model={model} visible={visible} />);
      return driver;
    },
    expectSingleLineText(expected: string) {
      byTestId(TEST_IDS.enqueuedInputText).should("have.text", expected);
      byTestId(TEST_IDS.enqueuedInputOverlay).should("have.css", "white-space", "nowrap");
      return driver;
    },
    expectMouseGlyphWithCount(expected: number) {
      byTestId(TEST_IDS.enqueuedInputMouse)
        .should("have.attr", "data-count", String(expected))
        .and("contain.text", String(expected));
      return driver;
    },
    expectOverflowGlyphWithCount(expected: number) {
      byTestId(TEST_IDS.enqueuedInputOverflow)
        .should("have.attr", "data-count", String(expected))
        .and("contain.text", String(expected));
      return driver;
    },
    expectNoMouseGlyph() {
      byTestId(TEST_IDS.enqueuedInputMouse).should("not.exist");
      return driver;
    },
    expectNoOverflowGlyph() {
      byTestId(TEST_IDS.enqueuedInputOverflow).should("not.exist");
      return driver;
    },
    expectHidden() {
      byTestId(TEST_IDS.enqueuedInputOverlay).should("not.exist");
      return driver;
    },
  };
  return driver;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("EnqueuedInputOverlay — presentation", () => {
  it("renders un-acked keyboard text inline on a single line", () => {
    // Given — the server has not acked the typed "HELLO WORLD"
    const model = anOverlayModel([text("HELLO WORLD")]);

    // When
    anOverlay(model)
      .mount()

      // Then
      .expectSingleLineText("HELLO WORLD")
      .expectNoMouseGlyph()
      .expectNoOverflowGlyph();
  });

  it("collapses consecutive mouse events into one glyph showing the event count", () => {
    // Given — three un-acked mouse events coalesced into one run
    const model = anOverlayModel([mouseRun(3)]);

    // When
    anOverlay(model)
      .mount()

      // Then
      .expectMouseGlyphWithCount(3);
  });

  it("collapses items beyond the line budget into a trailing overflow glyph with the overflow count", () => {
    // Given — "abc" fits the line, four more items overflow
    const model = anOverlayModel([text("abc")], 4);

    // When
    anOverlay(model)
      .mount()

      // Then
      .expectSingleLineText("abc")
      .expectOverflowGlyphWithCount(4);
  });

  it("renders nothing when the overlay is not visible", () => {
    // Given — un-acked input exists but the 500ms threshold has not elapsed
    const model = anOverlayModel([text("HELLO")]);

    // When
    anOverlay(model, false)
      .mount()

      // Then
      .expectHidden();
  });
});
