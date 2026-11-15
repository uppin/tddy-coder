import type { SessionEntry } from "tddy-client-web/gen/session_pb";
import { sessionDrawerLabel } from "./utils/sessionDrawerLabel";

export interface SessionNameEditorProps {
  session: SessionEntry;
  /**
   * Saves `displayName` as the session's name; resolves once the host accepted it, rejects with the
   * host's refusal. Absent when nothing can reach the session's host: the name is shown, not editable.
   */
  onRename?: (displayName: string) => Promise<void>;
}

// TODO(red): stub — renders the label only and never edits; the inline editor lands in the green phase.
export function SessionNameEditor({ session }: SessionNameEditorProps) {
  return (
    <span data-testid={`session-name-label-${session.sessionId}`} className="text-sm font-medium">
      {sessionDrawerLabel(session)}
    </span>
  );
}
