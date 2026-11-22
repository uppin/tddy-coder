/**
 * Land a range of waves, and the result per item; a refused propagation with the state and the actions
 * that fix it (`#workqueue` 14/14).
 *
 * Test ids: `work-queue-land-btn`, `work-queue-land-from`, `work-queue-land-to`, `work-queue-land-submit`,
 * `work-queue-land-result-${itemId}` (`data-outcome`: landed | stopped | skipped),
 * `work-item-propagation-refusal-${itemId}` (state and actions as text).
 */
export interface LandResultView {
  itemId: string;
  outcome: "landed" | "stopped" | "skipped";
  detail: string;
}

export function LandWaves(_props: { waveCount: number; onLand: (fromWave: number, toWave: number) => void; results: LandResultView[] }) {
  // TODO(n14 land-propagate): implement
  return null;
}

export function PropagationRefusal(_props: { itemId: string; state: string; actions: string[] }) {
  // TODO(n14 land-propagate): implement
  return null;
}
