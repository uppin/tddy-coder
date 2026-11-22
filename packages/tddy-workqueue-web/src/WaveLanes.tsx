/**
 * A work queue's items in wave lanes with each item's readiness, and the line check
 * (`#workqueue` 10/14).
 *
 * Test ids: `work-queue-wave-${n}` (lane), `work-queue-wave-${n}-item-${id}`, `work-item-readiness-${id}`
 * ("ready" or "waits on n2, n4"), `work-queue-line-check` (present only when out of order; names the
 * first item out of place).
 */
export interface WaveLanesProps {
  /** Waves, wave 1 first; each in derived-line order. */
  waves: { itemId: string; title: string; waitsOn: string[] }[][];
  /** The first item out of place in the current line, when it steps back a wave. */
  outOfOrderAt?: string;
}

export function WaveLanes(_props: WaveLanesProps) {
  // TODO(n10 waves-line): implement
  return null;
}
