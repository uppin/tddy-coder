import { inputClass, labelClass } from "./createSessionFormStyles";

export interface CreateSessionNameFieldProps {
  /** The name as typed; trimmed by the request builder, so a blank one starts the session unnamed. */
  displayName: string;
  setDisplayName: (displayName: string) => void;
}

/**
 * **Name** — the session's optional display name, sent trimmed as `StartSessionRequest.display_name`
 * so the drawer row and its notifications carry it from the start. Left blank, the session is
 * labelled by its worktree as before. The pane shows it only for a session whose launch the daemon
 * names (not a tool session, whose tddy-coder writes its own record, and not main). Test id
 * `create-session-name-input`.
 */
export function CreateSessionNameField({ displayName, setDisplayName }: CreateSessionNameFieldProps) {
  return (
    <div>
      <label className={labelClass} htmlFor="create-session-name">
        Name
      </label>
      <input
        id="create-session-name"
        data-testid="create-session-name-input"
        type="text"
        className={inputClass}
        placeholder="Optional — defaults to the worktree name"
        value={displayName}
        onChange={(e) => setDisplayName(e.target.value)}
      />
    </div>
  );
}
