/**
 * Land a range of waves, and the result per item; a refused propagation with the state and the actions
 * that fix it (`#workqueue` 14/14).
 *
 * Test ids: `work-queue-land-btn`, `work-queue-land-from`, `work-queue-land-to`, `work-queue-land-submit`,
 * `work-queue-land-result-${itemId}` (`data-outcome`: landed | stopped | skipped),
 * `work-item-propagation-refusal-${itemId}` (state and actions as text).
 */
import React, { useState } from "react";

export interface LandResultView {
  itemId: string;
  outcome: "landed" | "stopped" | "skipped";
  detail: string;
}

export function LandWaves(props: {
  waveCount: number;
  onLand: (fromWave: number, toWave: number) => void;
  results: LandResultView[];
}) {
  const [asking, setAsking] = useState(false);
  const [from, setFrom] = useState(1);
  const [to, setTo] = useState(Math.max(1, props.waveCount));
  const waves = Array.from({ length: props.waveCount }, (_, i) => i + 1);
  const options = waves.map((w) => (
    <option key={w} value={w}>
      {w}
    </option>
  ));
  return (
    <div data-testid="work-queue-land">
      <button type="button" data-testid="work-queue-land-btn" onClick={() => setAsking((open) => !open)}>
        Land wave
      </button>
      {asking && (
        <span>
          <select data-testid="work-queue-land-from" value={from} onChange={(e) => setFrom(Number(e.target.value))}>
            {options}
          </select>
          <select data-testid="work-queue-land-to" value={to} onChange={(e) => setTo(Number(e.target.value))}>
            {options}
          </select>
          <button type="button" data-testid="work-queue-land-submit" onClick={() => props.onLand(from, to)}>
            Land
          </button>
        </span>
      )}
      <ul>
        {props.results.map((r) => (
          <li key={r.itemId} data-testid={`work-queue-land-result-${r.itemId}`} data-outcome={r.outcome}>
            {r.itemId}: {r.outcome} — {r.detail}
          </li>
        ))}
      </ul>
    </div>
  );
}

export function PropagationRefusal(props: { itemId: string; state: string; actions: string[] }) {
  return (
    <div data-testid={`work-item-propagation-refusal-${props.itemId}`} role="alert">
      Not propagated: the local branch is {props.state}. To fix it: {props.actions.join(", then ")}.
    </div>
  );
}
