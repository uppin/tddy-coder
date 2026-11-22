/**
 * A work queue's armed schedules with each condition's state and detail (`#workqueue` 13/14), live
 * from the queue's status updates.
 *
 * Test ids: `work-queue-schedule-${itemId}` (`data-status`), `work-queue-schedule-${itemId}-${condition}`
 * (`data-state`: met | waiting | blocked; text = detail), `work-queue-schedule-${itemId}-held-up-on`.
 */
export interface ScheduleConditionView {
  name: string;
  state: "met" | "waiting" | "blocked";
  detail: string;
}

export interface ScheduleViewModel {
  itemId: string;
  kind: "merge" | "propagate";
  status: "met" | "waiting" | "blocked";
  heldUpOn?: string;
  conditions: ScheduleConditionView[];
}

export function SchedulesPanel(_props: { schedules: ScheduleViewModel[] }) {
  // TODO(n13 schedules): implement
  return null;
}
