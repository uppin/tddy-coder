/**
 * "Default for…" on a provider account's row: the harnesses a session start uses this account for
 * when the person names none. The choices are the harnesses a session can be started on; the daemon
 * refuses one the account cannot drive and says why, so no matrix is kept here.
 */

import { useState } from "react";
import { Button } from "tddy-ui-web";

/** One session type a provider account can be the default for, as the daemon served it. */
export interface HarnessOption {
  id: string;
  label: string;
}

export function ProviderAccountDefaultsControl({
  rowId,
  current,
  options,
  onSave,
}: {
  rowId: string;
  /** The harnesses the account is the default for now. */
  current: string[];
  /** The session types the daemon serves, from its `ListAgents` answer. */
  options: readonly HarnessOption[];
  onSave: (harnesses: string[]) => void;
}) {
  const [draft, setDraft] = useState<string[]>(current);
  // A default named for a harness the daemon no longer serves stays selectable, captioned by
  // its id, so an existing default is never silently dropped by opening this control.
  const allOptions = [
    ...options,
    ...current
      .filter((id) => !options.some((option) => option.id === id))
      .map((id) => ({ id, label: id })),
  ];
  return (
    <span className="flex items-center gap-2">
      <label className="text-xs text-muted-foreground" htmlFor={`${rowId}-default-for`}>
        Default for
      </label>
      <select
        id={`${rowId}-default-for`}
        multiple
        data-testid={`${rowId}-default-for`}
        className="rounded-md border border-border bg-background px-2 text-sm"
        value={draft}
        onChange={(event) =>
          setDraft(Array.from(event.target.selectedOptions, (option) => option.value))
        }
      >
        {allOptions.map((harness) => (
          <option key={harness.id} value={harness.id}>
            {harness.label}
          </option>
        ))}
      </select>
      <Button
        size="sm"
        variant="outline"
        data-testid={`${rowId}-default-for-save`}
        onClick={() => onSave(draft)}
      >
        Save default
      </Button>
    </span>
  );
}
