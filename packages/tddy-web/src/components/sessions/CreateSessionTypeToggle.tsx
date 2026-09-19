import type { SessionType } from "./createSessionRequest";

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
      <button
        type="button"
        data-testid="create-session-type-tool"
        aria-pressed={sessionType === "tool"}
        onClick={() => setSessionType("tool")}
        className={`px-3 py-1.5 rounded-md text-sm border transition-colors ${
          sessionType === "tool"
            ? "bg-primary text-primary-foreground border-primary"
            : "bg-background text-foreground border-input hover:bg-muted"
        }`}
      >
        Tool
      </button>
      <button
        type="button"
        data-testid="create-session-type-claude-cli"
        aria-pressed={sessionType === "claude-cli"}
        onClick={() => setSessionType("claude-cli")}
        className={`px-3 py-1.5 rounded-md text-sm border transition-colors ${
          sessionType === "claude-cli"
            ? "bg-primary text-primary-foreground border-primary"
            : "bg-background text-foreground border-input hover:bg-muted"
        }`}
      >
        Claude CLI
      </button>
      <button
        type="button"
        data-testid="create-session-type-cursor-cli"
        aria-pressed={sessionType === "cursor-cli"}
        onClick={() => setSessionType("cursor-cli")}
        className={`px-3 py-1.5 rounded-md text-sm border transition-colors ${
          sessionType === "cursor-cli"
            ? "bg-primary text-primary-foreground border-primary"
            : "bg-background text-foreground border-input hover:bg-muted"
        }`}
      >
        Cursor CLI
      </button>
    </div>
  );
}
