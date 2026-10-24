import type { SessionType } from "./createSessionRequest";

export interface CreateSessionTypeToggleProps {
  sessionType: SessionType;
  setSessionType: (sessionType: SessionType) => void;
  /**
   * Why a type cannot be chosen here, or `null` when it can. A type with a reason is shown
   * disabled, its reason listed under the toggle. Absent: every type can be chosen.
   */
  disabledReason?: (sessionType: SessionType) => string | null;
}

/** The session types the toggle offers, in order, with the label each button shows. */
const SESSION_TYPE_OPTIONS: { id: SessionType; label: string }[] = [
  { id: "tool", label: "Tool" },
  { id: "claude-cli", label: "Claude CLI" },
  { id: "cursor-cli", label: "Cursor CLI" },
  { id: "opencode-cli", label: "OpenCode" },
  { id: "muse-cli", label: "Muse Code" },
  { id: "warp-cli", label: "Warp" },
];

/** The id of the element saying why `sessionType` is disabled; the button is described by it. */
const reasonIdOf = (sessionType: SessionType) => `create-session-type-reason-${sessionType}`;

/**
 * Session type toggle — which backend the session is started as. Presentational: the selection
 * itself stays in `CreateSessionPane`, which every other field is derived from.
 */
export function CreateSessionTypeToggle({
  sessionType,
  setSessionType,
  disabledReason = () => null,
}: CreateSessionTypeToggleProps) {
  const options = SESSION_TYPE_OPTIONS.map((option) => ({ ...option, reason: disabledReason(option.id) }));
  const disabled = options.filter((option) => option.reason !== null);
  return (
    <>
      <div className="flex gap-2">
        {options.map(({ id, label, reason }) => (
          <button
            key={id}
            type="button"
            data-testid={`create-session-type-${id}`}
            aria-pressed={sessionType === id}
            aria-describedby={reason !== null ? reasonIdOf(id) : undefined}
            disabled={reason !== null}
            onClick={() => setSessionType(id)}
            className={`px-3 py-1.5 rounded-md text-sm border transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${
              sessionType === id
                ? "bg-primary text-primary-foreground border-primary"
                : "bg-background text-foreground border-input hover:bg-muted"
            }`}
          >
            {label}
          </button>
        ))}
      </div>
      {disabled.length > 0 && (
        <ul className="space-y-1 text-xs text-muted-foreground">
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
    </>
  );
}
