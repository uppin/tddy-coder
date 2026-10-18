# 2026-10-18 — The Code pane's hover card does not follow the preview while it scrolls

**Category:** Future enhancement
**Source:** `code-index-warm-on-demand` changeset — Technical Debt & Production Readiness

## What is left

The hover card (`CodeNavigationOverlays.tsx`, `useCardPosition`) is placed beside its identifier when
it opens and on each re-render of the card. It lives in the preview's positioned, **non-scrolling**
wrapper, so scrolling the preview with the card open leaves the card where it was while the identifier
moves away under it (or out of view). The card finds its identifier by `data-identifier-at` at every
layout, so a re-render puts it back in the right place — but nothing re-lays it out on scroll.

## Why it was deferred

The PRD's criterion was an anchored, bounded card that never covers the whole preview; following a
scroll was not asked for, and the natural fixes (a scroll listener that re-lays the card out, or moving
the card into the scrolling content) each carry their own trade-off — a listener re-measures on every
scroll frame, and moving the card into the scrolled content makes its bounds relative to the whole file
rather than the visible preview.

## What would close it

Either re-run `useCardPosition`'s layout on the preview's `scroll` events (throttled to animation
frames), closing the card when its identifier leaves the visible area, or render the card inside the
scrolled content and clamp it to the visible rect. Pin it with a `CodeHoverCard.cy.tsx` case that
scrolls the preview with the card open.
