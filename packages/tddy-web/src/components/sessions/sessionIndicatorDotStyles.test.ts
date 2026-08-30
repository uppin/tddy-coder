import { describe, expect, it } from "bun:test";
import { SESSION_INDICATOR_DOT_STYLES } from "./sessionIndicatorDotStyles";
import { SESSION_INDICATOR_BLINK_CLASS } from "./SessionIndicatorDot";

/**
 * The `working` dot's animation, and the accessibility guard on it.
 *
 * PRD: docs/ft/daemon/session-notifications.md (FR4, NFR2).
 *
 * Asserted against the stylesheet rather than a rendered dot's computed style: a headless Cypress
 * runner samples whatever animation frame it happens to catch, so an assertion on opacity there is
 * a coin toss. The rules themselves are what NFR2 is a claim about, and they are static.
 */

/** The declarations inside the `prefers-reduced-motion: reduce` block. */
function reducedMotionBlock(): string {
  const [, block] = SESSION_INDICATOR_DOT_STYLES.split("@media (prefers-reduced-motion: reduce)");
  return block ?? "";
}

describe("SESSION_INDICATOR_DOT_STYLES — the working dot's fade", () => {
  it("fades the dot from fully opaque to partly transparent and back", () => {
    // Given / When
    const keyframes = SESSION_INDICATOR_DOT_STYLES.split("@keyframes tddy-session-dot-blink")[1];

    // Then — a fade in and out, not a fade to nothing: a dot that reaches zero reads as absent
    // rather than busy on a row the operator is scanning past.
    expect(keyframes).toContain("0%, 100% { opacity: 1; }");
    expect(keyframes).toContain("50% { opacity: 0.25; }");
  });

  it("animates the same class the drawer puts on a working dot", () => {
    // Given — the class name lives in two places: this stylesheet, and the constant the dot renders
    // with. Renaming one and not the other yields a dot that silently never animates.
    // When / Then
    expect(SESSION_INDICATOR_DOT_STYLES).toContain(`.${SESSION_INDICATOR_BLINK_CLASS} {`);
  });

  // -------------------------------------------------------------------------
  // NFR2 — reduced motion
  // -------------------------------------------------------------------------

  it("carries a prefers-reduced-motion guard", () => {
    // Given / When / Then
    expect(SESSION_INDICATOR_DOT_STYLES).toContain("@media (prefers-reduced-motion: reduce)");
  });

  it("disables the animation for a viewer who asked for reduced motion", () => {
    // Given / When / Then
    expect(reducedMotionBlock()).toContain("animation: none;");
  });

  it("leaves the dot fully opaque when the animation is disabled", () => {
    // Given — stopping the animation mid-fade would freeze the dot at whatever opacity it held,
    // and a permanently half-faded green dot reads as a different state, not as a still one.
    // When / Then
    expect(reducedMotionBlock()).toContain("opacity: 1;");
  });

  it("guards the working dot specifically, not every dot in the drawer", () => {
    // Given / When / Then — the steady states have no animation to disable, and a blanket rule
    // would be a claim about dots this stylesheet does not own.
    expect(reducedMotionBlock()).toContain(`.${SESSION_INDICATOR_BLINK_CLASS} {`);
  });
});
