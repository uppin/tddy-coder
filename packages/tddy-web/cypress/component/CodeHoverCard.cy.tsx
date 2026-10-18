/**
 * The Code pane's hover card is a bounded floating card anchored to the hovered identifier: it opens
 * just below the symbol, is at most `min(40rem, pane width − 2rem)` wide and `20rem` tall, scrolls
 * its body while its References / Close actions stay in view, and renders the hover's markdown.
 *
 * Feature: docs/ft/web/1-WIP/PRD-2026-10-18-code-index-warm-on-demand.md (Web, Hover).
 *
 * Mounted through the whole pane over a `CodeSession` fake, so the card's position and size are the
 * ones the real layout gives it; the app stylesheet is loaded for the same reason.
 */

import "../../src/index.css";
import { anIndexReady, type CodeIndexHost } from "../support/rpc/codeSessionBackend";
import { aCodePaneOn } from "../support/drivers/worktreeCodePaneDriver";
import { worktreeCodePanePage } from "../support/pages/worktreeCodePanePage";
import {
  AREA_TYPE,
  THE_CALL_TO_AREA,
  aGeometryWorktreeHost,
} from "../support/fixtures/geometryWorktree";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const THE_DOC = "Computes the area of a rectangle.";

// A doc comment long enough, in both directions, to overflow any reasonable card: an unbroken
// 300-character path and 120 paragraphs.
const A_LONG_HOVER = [
  AREA_TYPE,
  `crate::${"very_long_module_name_".repeat(14)}area`,
  ...Array.from({ length: 120 }, (_, i) => `Paragraph ${i + 1} of the area documentation.`),
].join("\n\n");

const A_HOVER_WITH_A_HEADING = [
  AREA_TYPE,
  "",
  "## Examples",
  "",
  THE_DOC,
].join("\n");

const THE_BODY = "    width * height";

// What rust-analyzer answers for a function: its signature in a fenced rust block, then its docs.
const A_HOVER_WITH_A_FENCED_BLOCK = ["```rust", AREA_TYPE, THE_BODY, "```", "", THE_DOC].join("\n");

// A fence that is never closed: everything after it is code.
const A_HOVER_WITH_AN_UNCLOSED_FENCE = [THE_DOC, "", "```rust", AREA_TYPE, THE_BODY].join("\n");

/** The geometry worktree, whose loaded index answers the call to `area` with `hover`. */
function aHostHovering(hover: string): CodeIndexHost {
  return aGeometryWorktreeHost(hover).startingAt(anIndexReady());
}

// ---------------------------------------------------------------------------
// Geometry assertions
// ---------------------------------------------------------------------------

const rem = () => parseFloat(getComputedStyle(document.documentElement).fontSize);

/** The card's top sits just below the identifier's bottom, and the card spans the identifier. */
function expectTheHoverCardJustBelowTheCallToArea() {
  worktreeCodePanePage
    .identifier(THE_CALL_TO_AREA.line, THE_CALL_TO_AREA.column)
    .then(($identifier) => {
      const identifier = $identifier[0].getBoundingClientRect();
      worktreeCodePanePage.hoverCard().should(($card) => {
        const card = $card[0].getBoundingClientRect();
        // The gap between symbol and card is a styling choice of a few pixels, so only its range
        // is fixed: below the identifier, not covering it, and not drifting away from it.
        expect(card.top - identifier.bottom, "gap below the identifier").to.be.within(0, 8);
        expect(card.left, "card left edge").to.be.at.most(identifier.left);
        expect(card.right, "card right edge").to.be.at.least(identifier.right);
      });
    });
}

/** The card fits `min(40rem, preview width − 2rem)` × `20rem`; its body scrolls what does not fit. */
function expectTheHoverCardBounded() {
  worktreeCodePanePage.preview().then(($preview) => {
    const previewWidth = $preview[0].getBoundingClientRect().width;
    worktreeCodePanePage.hoverCard().should(($card) => {
      const card = $card[0].getBoundingClientRect();
      // Layout widths are fractional CSS pixels, so the bound is checked to within half a pixel.
      expect(card.width, "card width").to.be.at.most(Math.min(40 * rem(), previewWidth - 2 * rem()) + 0.5);
      expect(card.height, "card height").to.be.at.most(20 * rem() + 0.5);
    });
  });
  worktreeCodePanePage
    .hoverBody()
    .should("have.css", "overflow-y", "auto")
    .and(($body) => {
      expect($body[0].scrollHeight, "body content height").to.be.greaterThan($body[0].clientHeight);
    });
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 900);
});

it("the hover card opens beside the hovered identifier", () => {
  // Given main.rs previewed with the index loaded
  const pane = aCodePaneOn(aHostHovering(AREA_TYPE)).mount().previewing("src/main.rs");

  // When the pointer rests on the call to `area`
  pane.dwellOn(THE_CALL_TO_AREA);

  // Then its card opens just below it
  worktreeCodePanePage.hoverBody().should("have.text", AREA_TYPE);
  expectTheHoverCardJustBelowTheCallToArea();
});

it("a long hover stays within its bounds and keeps its actions visible", () => {
  // Given main.rs previewed in a narrow pane, where the width bound is the pane's, not 40rem
  const pane = aCodePaneOn(aHostHovering(A_LONG_HOVER))
    .mount({ width: 640, height: 800 })
    .previewing("src/main.rs");

  // When the pointer rests on the call to `area`, whose docs are very long
  pane.dwellOn(THE_CALL_TO_AREA);

  // Then the card stays within its bounds and scrolls its body
  worktreeCodePanePage.hoverBody().should("contain.text", "Paragraph 1 of the area documentation.");
  expectTheHoverCardBounded();
  // And its actions are in view
  worktreeCodePanePage.referencesAction().should("be.visible");
  worktreeCodePanePage.hoverClose().should("be.visible");
});

it("renders hover markdown headings as headings", () => {
  // Given main.rs previewed with the index loaded
  const pane = aCodePaneOn(aHostHovering(A_HOVER_WITH_A_HEADING)).mount().previewing("src/main.rs");

  // When the pointer rests on the call to `area`
  pane.dwellOn(THE_CALL_TO_AREA);

  // Then the heading is its own heading element, apart from the paragraphs around it
  worktreeCodePanePage.hoverBodyHeadings().should("have.length", 1).and("have.text", "Examples");
  worktreeCodePanePage.hoverBodyParagraphs().first().should("have.text", AREA_TYPE);
  worktreeCodePanePage.hoverBodyParagraphs().last().should("have.text", THE_DOC);
});

it("renders a fenced rust block as one code block, apart from the docs after it", () => {
  // Given main.rs previewed with the index loaded
  const pane = aCodePaneOn(aHostHovering(A_HOVER_WITH_A_FENCED_BLOCK))
    .mount()
    .previewing("src/main.rs");

  // When the pointer rests on the call to `area`
  pane.dwellOn(THE_CALL_TO_AREA);

  // Then the fenced lines are one code block, without their fences
  worktreeCodePanePage
    .hoverBodyCodeBlocks()
    .should("have.length", 1)
    .and("have.text", `${AREA_TYPE}\n${THE_BODY}`);
  // And the docs after it are a paragraph of their own
  worktreeCodePanePage.hoverBodyParagraphs().should("have.length", 1).and("have.text", THE_DOC);
});

it("renders everything after a fence that is never closed as code", () => {
  // Given main.rs previewed with the index loaded
  const pane = aCodePaneOn(aHostHovering(A_HOVER_WITH_AN_UNCLOSED_FENCE))
    .mount()
    .previewing("src/main.rs");

  // When the pointer rests on the call to `area`
  pane.dwellOn(THE_CALL_TO_AREA);

  // Then the docs before the fence are a paragraph, and every line after it is code
  worktreeCodePanePage.hoverBodyParagraphs().should("have.length", 1).and("have.text", THE_DOC);
  worktreeCodePanePage
    .hoverBodyCodeBlocks()
    .should("have.length", 1)
    .and("have.text", `${AREA_TYPE}\n${THE_BODY}`);
});
