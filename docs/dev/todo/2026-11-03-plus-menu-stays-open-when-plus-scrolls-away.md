# 2026-11-03 — The "+" menu stays open when its "+" scrolls out of the tab strip

**Category:** UX follow-up
**Source:** #session-tabs sweep

The `NewTerminalMenu` is portaled, so when the tab scroller moves and the "+" button that opened it leaves the scroller's visible box, the menu stays open, floating over content it no longer belongs to.

**Why deferred:** a UX nicety found during the session-tabs review, not a logic defect in the shipped feature, and not planned in any changeset. The developer chose, 2026-11-03, to defer it ("logic issues solved" in the sweep covers defects; this is not one).

**What would close it:** close the menu when its "+" leaves the tab scroller's visible box (an `IntersectionObserver` on the "+" against the scroller, or a scroll listener comparing rects), with a Cypress spec in `SessionTerminalTabsPlusPlacement.cy.tsx`.
