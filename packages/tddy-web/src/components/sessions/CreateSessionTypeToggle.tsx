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
   * Why the type is not the operator's to choose, or `null` when it is. Set on a start whose type
   * follows from what it is a start *of* — a project's main can only be a managed-capable session
   * type, so the toggle states the one correct type instead of offering a dead control.
   */
  disabledReason?: string | null;
}

/** Tool vs which of the daemon's CLI agents, as a segmented toggle. */
export function CreateSessionTypeToggle({
  options,
  sessionType,
  setSessionType,
  onSelect,
  disabledReason = null,
}: CreateSessionTypeToggleProps) {
  const disabled = disabledReason !== null;
  return (
    <div>
      <span className={labelClass}>Session type</span>
      <div
        role="group"
        aria-label="Session type"
        aria-disabled={disabled}
        title={disabledReason ?? undefined}
        data-testid="create-session-type-toggle"
        className="mt-1 flex flex-wrap gap-1"
      >
        {options.map(({ id, label }) => {
          const isActive = sessionType === id;
          return (
            <button
              key={id}
              type="button"
              data-testid={`create-session-type-${id}`}
              aria-pressed={isActive}
              disabled={disabled}
              onClick={() => {
                setSessionType(id);
                onSelect?.(id);
              }}
              className={
                isActive
                  ? "rounded-md bg-primary px-3 py-1.5 text-sm font-medium text-primary-foreground"
                  : "rounded-md border border-input bg-background px-3 py-1.5 text-sm hover:bg-accent"
              }
            >
              {label}
            </button>
          );
        })}
      </div>
      {disabledReason !== null && (
        <p className="mt-1 text-xs text-muted-foreground">{disabledReason}</p>
      )}
    </div>
  );
}
