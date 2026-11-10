import type { SessionTypeOption } from "./cliHarnessOptions";
import { labelClass } from "./createSessionFormStyles";

export interface CreateSessionTypeToggleProps {
  /** The session types the target daemon offers, from the agents it served. */
  options: readonly SessionTypeOption[];
  sessionType: string;
  setSessionType: (sessionType: string) => void;
  /** Set together with `sessionType`: the change of type itself must also discard the spawn
   *  shape recorded for it, so the parent wires both here. */
  onSelect?: (sessionType: string) => void;
  /**
   * Why a type cannot be chosen here, per type, or absent when every type can be chosen.
   * Set on a start whose types follow from what it is a start *of* — a project's main can
   * only be a main-capable session type, so each other type is disabled with its reason
   * stated beside it instead of offering a dead control.
   */
  disabledReason?: (sessionType: string) => string | null;
}

/** The id of the element saying why `sessionType` is disabled; the button is described by it. */
const reasonIdOf = (sessionType: string) => `create-session-type-reason-${sessionType}`;

/** Tool vs which of the daemon's CLI agents, as a segmented toggle. */
export function CreateSessionTypeToggle({
  options,
  sessionType,
  setSessionType,
  onSelect,
  disabledReason = () => null,
}: CreateSessionTypeToggleProps) {
  const withReasons = options.map((option) => ({
    ...option,
    reason: disabledReason(option.id),
  }));
  const disabled = withReasons.filter((option) => option.reason !== null);
  return (
    <div>
      <span className={labelClass}>Session type</span>
      <div
        role="group"
        aria-label="Session type"
        data-testid="create-session-type-toggle"
        className="mt-1 flex flex-wrap gap-1"
      >
        {withReasons.map(({ id, label, reason }) => {
          const isActive = sessionType === id;
          return (
            <button
              key={id}
              type="button"
              data-testid={`create-session-type-${id}`}
              aria-pressed={isActive}
              aria-describedby={reason !== null ? reasonIdOf(id) : undefined}
              disabled={reason !== null}
              onClick={() => {
                setSessionType(id);
                onSelect?.(id);
              }}
              className={
                isActive
                  ? "rounded-md bg-primary px-3 py-1.5 text-sm font-medium text-primary-foreground"
                  : "rounded-md border border-input bg-background px-3 py-1.5 text-sm hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
              }
            >
              {label}
            </button>
          );
        })}
      </div>
      {disabled.length > 0 && (
        <ul className="mt-1 space-y-1 text-xs text-muted-foreground">
          {disabled.map(({ id, label, reason }) => (
            <li key={id}>
              <span className="font-medium">{label}:</span>{" "}
              <span id={reasonIdOf(id)} data-testid={reasonIdOf(id)}>
                {reason}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
