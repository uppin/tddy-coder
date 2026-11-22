import React from "react";

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

export function WaveLanes({ waves, outOfOrderAt }: WaveLanesProps) {
  return (
    <div className="flex flex-col gap-3">
      {outOfOrderAt !== undefined && (
        <div
          data-testid="work-queue-line-check"
          role="alert"
          className="rounded-md bg-amber-100 px-3 py-2 text-sm text-amber-800 dark:bg-amber-900 dark:text-amber-100"
        >
          The line steps back a wave at {outOfOrderAt}.
        </div>
      )}
      {waves.map((wave, index) => {
        const lane = index + 1;
        return (
          <section
            key={lane}
            data-testid={`work-queue-wave-${lane}`}
            className="flex flex-col gap-2 rounded-md border p-2"
          >
            <h3 className="text-xs font-medium text-muted-foreground">Wave {lane}</h3>
            {wave.map((item) => (
              <div
                key={item.itemId}
                data-testid={`work-queue-wave-${lane}-item-${item.itemId}`}
                className="flex items-center justify-between gap-2 text-sm"
              >
                <span className="truncate">
                  {item.itemId} {item.title}
                </span>
                <span
                  data-testid={`work-item-readiness-${item.itemId}`}
                  className="flex-shrink-0 rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground"
                >
                  {item.waitsOn.length === 0 ? "ready" : `waits on ${item.waitsOn.join(", ")}`}
                </span>
              </div>
            ))}
          </section>
        );
      })}
    </div>
  );
}
