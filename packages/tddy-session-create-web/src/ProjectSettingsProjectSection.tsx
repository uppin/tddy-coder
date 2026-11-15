import React, { useEffect, useState } from "react";
import { ProjectService, type ProjectEntry } from "tddy-client-web/gen/project_pb";
import { rpcFailureText, useDaemonClient } from "tddy-client-web";
import { Button } from "tddy-ui-web";
import { inputClass, labelClass } from "./createSessionFormStyles";

/**
 * The project's name in its settings pane: a field started from the registry row and **Save**,
 * which sends `UpdateProject`. The daemon renames the project on every host that owns it, so the
 * request names no host. A refusal is shown in place and the typed name is kept.
 *
 * Test ids: `sessions-project-settings-name-<projectId>`, `…-name-save-<projectId>`,
 * `…-name-saved-<projectId>`, `…-name-error-<projectId>`.
 */
export function ProjectNameSection(props: {
  projectId: string;
  name: string;
  sessionToken: string;
  /** The rename was applied: the answer's row, so the listing can refresh. */
  onProjectChanged?: (project: ProjectEntry) => void;
}): React.ReactElement {
  const { projectId, sessionToken, onProjectChanged } = props;
  const projectClient = useDaemonClient(ProjectService);
  const [name, setName] = useState(props.name);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [saving, setSaving] = useState(false);

  // A refreshed listing (after this rename, or another client's) restarts the field from the row.
  useEffect(() => setName(props.name), [props.name]);

  const save = async () => {
    if (!projectClient || saving) return;
    setSaving(true);
    setError(null);
    setSaved(false);
    try {
      const resp = await projectClient.updateProject({ sessionToken, projectId, name });
      setSaved(true);
      if (resp.project) onProjectChanged?.(resp.project);
    } catch (err) {
      setError(rpcFailureText(err));
    } finally {
      setSaving(false);
    }
  };

  const fieldId = `project-settings-name-${projectId}`;
  return (
    <div className="mb-4">
      <label className={labelClass} htmlFor={fieldId}>Name</label>
      <div className="flex gap-2">
        <input
          id={fieldId}
          data-testid={`sessions-project-settings-name-${projectId}`}
          className={inputClass}
          value={name}
          onChange={(e) => {
            setName(e.target.value);
            setSaved(false);
          }}
        />
        <Button
          type="button"
          data-testid={`sessions-project-settings-name-save-${projectId}`}
          disabled={saving || !projectClient}
          onClick={() => void save()}
        >
          Save
        </Button>
      </div>
      {error !== null && (
        <p data-testid={`sessions-project-settings-name-error-${projectId}`} className="mt-2 text-sm text-destructive">{error}</p>
      )}
      {saved && (
        <p data-testid={`sessions-project-settings-name-saved-${projectId}`} className="mt-2 text-sm text-muted-foreground">Saved</p>
      )}
    </div>
  );
}

/**
 * **Delete project**, behind an inline confirmation that says the project's data stays on disk and
 * that it can be restored from the Projects screen. Confirming sends `DeleteProject` — a soft
 * delete the daemon applies on every host that owns the project. A refusal (such as a session
 * still running) is shown in place, beside the confirmation.
 *
 * Test ids: `sessions-project-settings-delete-<projectId>`, `…-delete-confirm-<projectId>`,
 * `…-delete-confirm-yes-<projectId>`, `…-delete-cancel-<projectId>`, `…-delete-error-<projectId>`.
 */
export function ProjectDeleteSection(props: {
  projectId: string;
  name: string;
  sessionToken: string;
  /** The project was deleted. */
  onDeleted?: (projectId: string) => void;
}): React.ReactElement {
  const { projectId, name, sessionToken, onDeleted } = props;
  const projectClient = useDaemonClient(ProjectService);
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);

  const confirm = async () => {
    if (!projectClient || deleting) return;
    setDeleting(true);
    setError(null);
    try {
      await projectClient.deleteProject({ sessionToken, projectId });
      setConfirming(false);
      onDeleted?.(projectId);
    } catch (err) {
      setError(rpcFailureText(err));
    } finally {
      setDeleting(false);
    }
  };

  return (
    <div className="mt-4 border-t border-border pt-4">
      {confirming ? (
        <div
          data-testid={`sessions-project-settings-delete-confirm-${projectId}`}
          role="alertdialog"
          aria-label={`Delete ${name}`}
          className="rounded-md border border-destructive/40 p-3"
        >
          <p className="mb-3 text-sm">
            Delete <span className="font-medium">{name}</span>? Its main checkout, worktrees, sessions
            and stacks are kept on disk. It leaves the drawer and the pickers, and you can bring it
            back with <span className="font-medium">Restore</span> from the Projects screen.
          </p>
          <div className="flex justify-end gap-2">
            <Button
              type="button"
              variant="secondary"
              data-testid={`sessions-project-settings-delete-cancel-${projectId}`}
              disabled={deleting}
              onClick={() => {
                setConfirming(false);
                setError(null);
              }}
            >
              Cancel
            </Button>
            <Button
              type="button"
              variant="destructive"
              data-testid={`sessions-project-settings-delete-confirm-yes-${projectId}`}
              disabled={deleting || !projectClient}
              onClick={() => void confirm()}
            >
              Delete project
            </Button>
          </div>
        </div>
      ) : (
        <Button
          type="button"
          variant="destructive"
          data-testid={`sessions-project-settings-delete-${projectId}`}
          onClick={() => setConfirming(true)}
        >
          Delete project
        </Button>
      )}
      {error !== null && (
        <p data-testid={`sessions-project-settings-delete-error-${projectId}`} className="mt-2 text-sm text-destructive">{error}</p>
      )}
    </div>
  );
}
