import type { SessionType } from "./createSessionRequest";

export interface CreateSessionTypeToggleProps {
  sessionType: SessionType;
  setSessionType: (sessionType: SessionType) => void;
}

/** The session types the toggle offers, in order, with the label each button shows. */
const SESSION_TYPE_OPTIONS: { id: SessionType; label: string }[] = [
  { id: "tool", label: "Tool" },
  { id: "claude-cli", label: "Claude CLI" },
  { id: "cursor-cli", label: "Cursor CLI" },
  { id: "opencode-cli", label: "OpenCode" },
  { id: "muse-cli", label: "Muse Code" },
];

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
