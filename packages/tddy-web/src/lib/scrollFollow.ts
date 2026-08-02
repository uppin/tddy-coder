/**
 * Viewport arithmetic for a tail-first, auto-following transcript.
 *
 * Three decisions, kept pure and away from the DOM so they can be reasoned about (and tested)
 * without a rendered scroll container: when a viewport counts as *following* the newest entry, when
 * it is near enough to the start of the loaded range to page older history in, and where the scroll
 * offset must land once that older page has been prepended.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-02-activities-tail-first-autoscroll.md § Following,
 * § Paging backwards.
 */

/** How close to the bottom (px) still counts as following the newest entry. A partially-scrolled row
 *  or a fractional offset lands inside this routinely, and treating those as "the reader scrolled
 *  away" would stop the transcript following for no gesture at all. */
export const PIN_THRESHOLD_PX = 32;

/** How close to the top (px) counts as reaching the start of the loaded range, and so triggers the
 *  fetch of the page before it. Wide enough that the page is in flight before the reader hits the
 *  hard stop. */
export const NEAR_TOP_THRESHOLD_PX = 64;

/** The three numbers a scroll container reports about itself. Taken as a plain record rather than an
 *  element so the arithmetic stays independent of the DOM. */
export interface ViewportMetrics {
  readonly scrollTop: number;
  readonly scrollHeight: number;
  readonly clientHeight: number;
}

/**
 * Is the viewport following the newest entry — i.e. within {@link PIN_THRESHOLD_PX} of the bottom?
 *
 * Content shorter than the viewport makes the remaining travel negative, which reads as pinned: a
 * transcript that fits shows its newest entry by definition, and there is nothing to scroll.
 */
export function isPinnedToBottom(viewport: ViewportMetrics): boolean {
  const remaining = viewport.scrollHeight - viewport.clientHeight - viewport.scrollTop;
  return remaining <= PIN_THRESHOLD_PX;
}

/** Has the viewport reached the start of the loaded range — within {@link NEAR_TOP_THRESHOLD_PX} of
 *  the top? */
export function isNearTop(viewport: Pick<ViewportMetrics, "scrollTop">): boolean {
  return viewport.scrollTop <= NEAR_TOP_THRESHOLD_PX;
}

/**
 * The offset that keeps the read position after an older page has been prepended: the entry the
 * operator was looking at stays under the same pixel, because everything above it grew by exactly
 * the prepended content's height.
 *
 * A page that added no height (nothing renderable resolved) leaves the offset alone rather than
 * nudging a reader who asked for nothing.
 */
export function scrollTopAfterPrepend(args: {
  readonly scrollTop: number;
  readonly previousScrollHeight: number;
  readonly nextScrollHeight: number;
}): number {
  const grew = Math.max(0, args.nextScrollHeight - args.previousScrollHeight);
  return args.scrollTop + grew;
}
