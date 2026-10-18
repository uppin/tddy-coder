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
import { aCodeIndexHost, anIndexReady, type CodeIndexHost } from "../support/rpc/codeSessionBackend";
import { aCodePaneOn } from "../support/drivers/worktreeCodePaneDriver";
import { worktreeCodePanePage } from "../support/pages/worktreeCodePanePage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

// `src/main.rs` calls `geometry::area` on line 4; the call's `area` starts at byte column 26.
const MAIN_RS = [
  "mod geometry;",
  "",
  "fn main() {",
  "    let area = geometry::area(3, 4);",
  '    println!("{area}");',
  "}",
  "",
].join("\n");
const THE_CALL_TO_AREA = { line: 4, column: 26 };

const AREA_TYPE = "fn area(width: u32, height: u32) -> u32";

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
  "Computes the area of a rectangle.",
].join("\n");

/** A worktree holding `src/main.rs`, whose loaded index answers the call to `area` with `hover`. */
function aHostHovering(hover: string): CodeIndexHost {
  return aCodeIndexHost()
    .withFile("src/main.rs", MAIN_RS)
    .startingAt(anIndexReady())
    .knowing({ at: { relPath: "src/main.rs", ...THE_CALL_TO_AREA }, hover });
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
  worktreeCodePanePage.hoverBody().should("contain.text", AREA_TYPE);
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
  worktreeCodePanePage
    .hoverBodyParagraphs()
    .last()
    .should("have.text", "Computes the area of a rectangle.");
});
