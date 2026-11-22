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

function ConditionRow({ itemId, condition }: { itemId: string; condition: ScheduleConditionView }) {
  return (
    <li
      data-testid={`work-queue-schedule-${itemId}-${condition.name}`}
      data-state={condition.state}
    >
      <strong>{condition.name}</strong> — {condition.detail}
    </li>
  );
}

export function SchedulesPanel({ schedules }: { schedules: ScheduleViewModel[] }) {
  if (schedules.length === 0) {
    return <div data-testid="work-queue-schedules-empty">No armed merges or propagations.</div>;
  }
  return (
    <section data-testid="work-queue-schedules">
      {schedules.map((schedule) => (
        <div
          key={schedule.itemId}
          data-testid={`work-queue-schedule-${schedule.itemId}`}
          data-status={schedule.status}
        >
          <h4>
            {schedule.itemId} — {schedule.kind}
          </h4>
          {schedule.heldUpOn ? (
            <div data-testid={`work-queue-schedule-${schedule.itemId}-held-up-on`}>
              held up on {schedule.heldUpOn}
            </div>
          ) : null}
          <ul>
            {schedule.conditions.map((condition) => (
              <ConditionRow key={condition.name} itemId={schedule.itemId} condition={condition} />
            ))}
          </ul>
        </div>
      ))}
    </section>
  );
}
