import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { Pencil } from "lucide-react";
import { Button, InlineError, cn } from "tddy-ui-web";
import { rpcFailureText } from "tddy-client-web";
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

/** The editor's draft while it is open; `null` while the label is shown. */
type Draft = { text: string; saving: boolean; error: string | null };

/**
 * The session's name, editable in place — the top of the inspector's Details tab.
 *
 * Shows the name, or the derived label (`sessionDrawerLabel`) muted while the session is unnamed.
 * "Rename", or a double-click on the label, opens a text input prefilled with the current name —
 * empty when unnamed, never the derived label. **Enter** saves the trimmed text; an emptied input
 * saves `""`, which clears the name back to the derived label. **Escape** cancels, and so does
 * **blur**: leaving the field is never a save, so a stray click cannot rename a session.
 *
 * While the save is in flight the input is disabled (a blur then is the browser dropping focus from
 * a disabled field, not the operator leaving, and is ignored). A refusal is shown beside the input,
 * which stays open with the operator's text and takes focus back.
 */
export function SessionNameEditor({ session, onRename }: SessionNameEditorProps) {
  const { sessionId } = session;
  const name = session.displayName.trim();
  const [draft, setDraft] = useState<Draft | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const editing = draft !== null;
  const saving = draft?.saving ?? false;

  // Focus follows the input: when it opens, and when a refused save hands it back enabled.
  useEffect(() => {
    if (editing && !saving) inputRef.current?.focus();
  }, [editing, saving]);

  const open = () => {
    if (onRename) setDraft({ text: name, saving: false, error: null });
  };
  const cancel = () => setDraft(null);

  const save = (text: string) => {
    if (!onRename) return;
    setDraft({ text, saving: true, error: null });
    onRename(text.trim()).then(
      () => setDraft(null),
      (err: unknown) => setDraft({ text, saving: false, error: rpcFailureText(err) }),
    );
  };

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (draft === null || draft.saving || e.nativeEvent.isComposing) return;
    if (e.key === "Enter") {
      e.preventDefault();
      save(draft.text);
    } else if (e.key === "Escape") {
      // Kept here: an Escape that closes the editor must not also close the drawer around it.
      e.preventDefault();
      e.stopPropagation();
      cancel();
    }
  };

  if (draft !== null) {
    return (
      <div className="flex flex-col gap-1">
        <input
          ref={inputRef}
          data-testid={`session-name-input-${sessionId}`}
          type="text"
          aria-label="Session name"
          placeholder={sessionDrawerLabel(session)}
          value={draft.text}
          disabled={draft.saving}
          onChange={(e) => setDraft({ ...draft, text: e.target.value, error: null })}
          onKeyDown={onKeyDown}
          onBlur={() => {
            if (!draft.saving) cancel();
          }}
          className="border border-border rounded px-2 py-1 text-sm bg-background disabled:opacity-60"
        />
        <InlineError
          message={draft.error}
          testId={`session-name-error-${sessionId}`}
          className="text-xs"
        />
      </div>
    );
  }

  return (
    <div className="flex items-center gap-2 min-w-0">
      <span
        data-testid={`session-name-label-${sessionId}`}
        data-unnamed={name === "" ? "true" : undefined}
        title={onRename ? "Double-click to rename" : undefined}
        onDoubleClick={open}
        className={cn(
          "text-sm font-medium truncate",
          name === "" && "text-muted-foreground font-normal",
        )}
      >
        {sessionDrawerLabel(session)}
      </span>
      {onRename && (
        <Button
          data-testid={`session-name-rename-${sessionId}`}
          variant="ghost"
          size="xs"
          onClick={open}
        >
          <Pencil />
          Rename
        </Button>
      )}
    </div>
  );
}
