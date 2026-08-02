/**
 * Unit tests — the viewport arithmetic behind the transcript's tail-first scrolling.
 *
 * The rendered behaviour (opens at the newest entry, follows while pinned, never yanks a reader,
 * pages backwards without moving the read position) is pinned by
 * `cypress/component/ActivitiesTailScrollAcceptance.cy.tsx`. These specs pin the three decisions
 * underneath it exhaustively, so the component specs can stay behavioural rather than combinatorial:
 * when a viewport counts as following, when it counts as near enough to the top to page, and where
 * the scroll offset must land after an older page is prepended.
 *
 * PRD: docs/ft/web/agent-activity-pane.md § Tail-first — opening and following, § Paging
 * backwards.
 */

import { describe, expect, it } from "bun:test";
import { isNearTop, isPinnedToBottom, scrollTopAfterPrepend } from "./scrollFollow";

/** A viewport 300px tall over 1000px of content: 700px of travel, bottom at scrollTop 700. */
function aScrolledViewport(scrollTop: number) {
  return { scrollTop, scrollHeight: 1000, clientHeight: 300 };
}

describe("isPinnedToBottom", () => {
  it("treats a viewport resting at the very bottom as following the newest entry", () => {
    // Given — the offset the transcript opens at
    const viewport = aScrolledViewport(700);

    // When / Then
    expect(isPinnedToBottom(viewport)).toBe(true);
  });

  it("treats a viewport within the pin threshold of the bottom as still following", () => {
    // Given — 32px short of the bottom: the threshold itself, which a partially-scrolled row or a
    // fractional offset lands on routinely
    const viewport = aScrolledViewport(668);

    // When / Then
    expect(isPinnedToBottom(viewport)).toBe(true);
  });

  it("treats a viewport scrolled past the pin threshold as detached", () => {
    // Given — one pixel beyond the threshold: the reader has deliberately moved away
    const viewport = aScrolledViewport(667);

    // When / Then
    expect(isPinnedToBottom(viewport)).toBe(false);
  });

  it("treats content shorter than the viewport as following, since there is nothing to scroll", () => {
    // Given — 120px of content in a 300px viewport, which can never leave scrollTop 0
    const viewport = { scrollTop: 0, scrollHeight: 120, clientHeight: 300 };

    // When / Then — a transcript that fits shows its newest entry by definition
    expect(isPinnedToBottom(viewport)).toBe(true);
  });
});

describe("isNearTop", () => {
  it("treats a viewport within the paging threshold of the top as reaching the loaded range's start", () => {
    // Given — 64px from the top: the threshold that triggers the older-page fetch
    const viewport = aScrolledViewport(64);

    // When / Then
    expect(isNearTop(viewport)).toBe(true);
  });

  it("treats a viewport below the paging threshold as still inside the loaded range", () => {
    // Given — one pixel further down, where no fetch should be issued
    const viewport = aScrolledViewport(65);

    // When / Then
    expect(isNearTop(viewport)).toBe(false);
  });
});

describe("scrollTopAfterPrepend", () => {
  it("keeps the read position by adding the prepended content's height to the offset", () => {
    // Given — a reader at the top of the loaded range (offset 0) when 1200px of older entries land
    // above them, growing the content from 1000px to 2200px

    // When
    const offset = scrollTopAfterPrepend({
      scrollTop: 0,
      previousScrollHeight: 1000,
      nextScrollHeight: 2200,
    });

    // Then — the offset moves by exactly the prepended height, so the entry the operator was
    // reading stays under the same pixel instead of being pushed off screen
    expect(offset).toBe(1200);
  });

  it("leaves the offset alone when the prepended page added no height", () => {
    // Given — a page that resolved to nothing renderable: the content did not grow

    // When
    const offset = scrollTopAfterPrepend({
      scrollTop: 240,
      previousScrollHeight: 1000,
      nextScrollHeight: 1000,
    });

    // Then — no compensation, rather than a nudge the reader did not ask for
    expect(offset).toBe(240);
  });
});
