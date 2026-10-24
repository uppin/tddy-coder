import type { SessionType } from "./createSessionRequest";
import { SESSION_TYPE_OPTIONS } from "./cliHarnessOptions";

export interface CreateSessionTypeToggleProps {
  sessionType: SessionType;
  setSessionType: (sessionType: SessionType) => void;
}

/**
 * Session type toggle — which backend the session is started as. Presentational: the selection
 * itself stays in `CreateSessionPane`, which every other field is derived from.
 */
export function CreateSessionTypeToggle({
  sessionType,
  setSessionType,
}: CreateSessionTypeToggleProps) {
  return (
    <div className="flex gap-2">
      {SESSION_TYPE_OPTIONS.map(({ id, label }) => (
        <button
          key={id}
          type="button"
          data-testid={`create-session-type-${id}`}
          aria-pressed={sessionType === id}
          onClick={() => setSessionType(id)}
          className={`px-3 py-1.5 rounded-md text-sm border transition-colors ${
            sessionType === id
              ? "bg-primary text-primary-foreground border-primary"
              : "bg-background text-foreground border-input hover:bg-muted"
          }`}
        >
          {label}
        </button>
      ))}
    </div>
  );
}
