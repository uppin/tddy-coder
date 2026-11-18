import { useEffect, useState } from "react";
import { useActionFailure } from "tddy-client-web";
import { InlineError } from "tddy-ui-web";
import type { ProjectGroup } from "./ProjectCard";

/**
 * **Edit** on a project card: an inline form started from the registry row — the name and git URL,
 * which are properties of the logical project and fan out to every host, and one checkout-path field
 * per host, which moves that host's checkout alone (`UpdateProject` with `local_only`). A refused
 * save is shown in place and the typed values are kept.
 *
 * Test ids: `project-edit-toggle-<projectId>`, `project-edit-name-<projectId>`,
 * `project-edit-git-url-<projectId>`, `project-edit-save-<projectId>`, `…-saved-<projectId>`,
 * `project-edit-path-<projectId>-<daemonInstanceId>`, `…-path-save-<projectId>-<daemonInstanceId>`,
 * `project-edit-path-error-<projectId>-<daemonInstanceId>`.
 */
export function ProjectEditSection(props: {
  group: ProjectGroup;
  /** Save the logical project's name and git URL; rejects with the daemon's reason. */
  onUpdateProject: (input: { projectId: string; name: string; gitUrl: string }) => Promise<void>;
  /** Move one host's checkout path (`UpdateProject` with `local_only`); rejects with the reason. */
  onMoveProjectCheckout: (input: {
    projectId: string;
    name: string;
    mainRepoPath: string;
    daemonInstanceId: string;
  }) => Promise<void>;
}): React.ReactElement {
  const { group, onUpdateProject, onMoveProjectCheckout } = props;
  const failure = useActionFailure();
  const [name, setName] = useState(group.name);
  const [gitUrl, setGitUrl] = useState(group.gitUrl);
  const [saved, setSaved] = useState(false);

  // A refreshed listing (after this save, or another client's) restarts the fields from the row.
  useEffect(() => {
    setName(group.name);
    setGitUrl(group.gitUrl);
  }, [group.name, group.gitUrl]);

  const save = async () => {
    setSaved(false);
    const ok = await failure.run(() =>
      onUpdateProject({ projectId: group.projectId, name: name.trim(), gitUrl: gitUrl.trim() }),
    );
    setSaved(ok);
  };

  const moveCheckout = (daemonInstanceId: string, mainRepoPath: string) =>
    failure.run(() =>
      onMoveProjectCheckout({
        projectId: group.projectId,
        name: group.name,
        mainRepoPath,
        daemonInstanceId,
      }),
    );

  return (
    <div className="mb-3 rounded-md border border-border p-3" data-testid={`project-edit-form-${group.projectId}`}>
      <div className="mb-2">
        <label
          className="mb-1 block text-sm text-muted-foreground"
          htmlFor={`project-edit-name-${group.projectId}`}
        >
          Name
        </label>
        <input
          id={`project-edit-name-${group.projectId}`}
          data-testid={`project-edit-name-${group.projectId}`}
          className="w-full rounded border border-border px-2 py-1"
          value={name}
          onChange={(e) => {
            setName(e.target.value);
            setSaved(false);
          }}
        />
      </div>
      <div className="mb-2">
        <label
          className="mb-1 block text-sm text-muted-foreground"
          htmlFor={`project-edit-git-url-${group.projectId}`}
        >
          Git URL
        </label>
        <input
          id={`project-edit-git-url-${group.projectId}`}
          data-testid={`project-edit-git-url-${group.projectId}`}
          className="w-full rounded border border-border px-2 py-1 font-mono text-xs"
          value={gitUrl}
          onChange={(e) => {
            setGitUrl(e.target.value);
            setSaved(false);
          }}
        />
      </div>
      <button
        type="button"
        data-testid={`project-edit-save-${group.projectId}`}
        className="rounded-md border border-border px-3 py-1 text-sm font-medium"
        disabled={failure.pending}
        onClick={() => void save()}
      >
        {failure.pending ? "Saving…" : "Save"}
      </button>
      {saved && (
        <span data-testid={`project-edit-saved-${group.projectId}`} className="ml-2 text-sm text-muted-foreground">
          Saved
        </span>
      )}
      <InlineError testId={`project-edit-error-${group.projectId}`} message={failure.error} className="mt-2" />

      <div className="mt-3 border-t border-border pt-2">
        <div className="mb-1 text-sm text-muted-foreground">
          Checkout path per host — moving one host's checkout moves only that host's row
        </div>
        {group.hosts.map((host) => (
          <CheckoutPathField
            key={host.daemonInstanceId}
            projectId={group.projectId}
            daemonInstanceId={host.daemonInstanceId}
            currentPath={host.mainRepoPath}
            onMove={moveCheckout}
          />
        ))}
      </div>
    </div>
  );
}

function CheckoutPathField(props: {
  projectId: string;
  daemonInstanceId: string;
  currentPath: string;
  onMove: (daemonInstanceId: string, mainRepoPath: string) => Promise<void>;
}): React.ReactElement {
  const { projectId, daemonInstanceId, currentPath, onMove } = props;
  const failure = useActionFailure();
  const [path, setPath] = useState(currentPath);

  useEffect(() => setPath(currentPath), [currentPath]);

  return (
    <div className="mb-2">
      <div className="mb-1 flex items-center gap-2 text-xs text-muted-foreground">
        <span className="font-medium text-foreground">{daemonInstanceId}</span>
      </div>
      <div className="flex gap-2">
        <input
          data-testid={`project-edit-path-${projectId}-${daemonInstanceId}`}
          className="flex-1 rounded border border-border px-2 py-1 font-mono text-xs"
          value={path}
          onChange={(e) => setPath(e.target.value)}
        />
        <button
          type="button"
          data-testid={`project-edit-path-save-${projectId}-${daemonInstanceId}`}
          className="rounded-md border border-border px-3 py-1 text-sm"
          disabled={failure.pending || path === currentPath}
          onClick={() => void failure.run(() => onMove(daemonInstanceId, path.trim()))}
        >
          {failure.pending ? "Moving…" : "Move"}
        </button>
      </div>
      <InlineError
        testId={`project-edit-path-error-${projectId}-${daemonInstanceId}`}
        message={failure.error}
        className="mt-1"
      />
    </div>
  );
}

/**
 * **Delete project** on a project card, behind an inline confirmation that says the project's data
 * stays on disk and that it can be restored from this screen. Confirming sends `DeleteProject` — a
 * soft delete the daemon applies on every host that owns the project. A refusal (such as a session
 * still running) is shown in place.
 *
 * Test ids: `project-delete-<projectId>`, `…-confirm-<projectId>`, `…-confirm-yes-<projectId>`,
 * `…-cancel-<projectId>`, `project-delete-error-<projectId>`.
 */
export function ProjectDeleteSection(props: {
  group: ProjectGroup;
  /** Delete the project (soft); rejects with the daemon's reason. */
  onDeleteProject: (projectId: string) => Promise<void>;
}): React.ReactElement {
  const { group, onDeleteProject } = props;
  const failure = useActionFailure();
  const [confirming, setConfirming] = useState(false);

  if (confirming) {
    return (
      <div
        data-testid={`project-delete-confirm-${group.projectId}`}
        role="alertdialog"
        aria-label={`Delete ${group.name}`}
        className="mt-3 rounded-md border border-destructive/40 p-3"
      >
        <p className="mb-3 text-sm">
          Delete <span className="font-medium">{group.name}</span>? Its main checkout, worktrees,
          sessions and stacks are kept on disk. It leaves the screen and the pickers, and you can
          bring it back with <span className="font-medium">Restore</span> under Show deleted projects.
        </p>
        <div className="flex justify-end gap-2">
          <button
            type="button"
            data-testid={`project-delete-cancel-${group.projectId}`}
            className="rounded-md border border-border px-3 py-1 text-sm"
            onClick={() => setConfirming(false)}
          >
            Cancel
          </button>
          <button
            type="button"
            data-testid={`project-delete-confirm-yes-${group.projectId}`}
            className="rounded-md border border-destructive px-3 py-1 text-sm font-medium text-destructive"
            disabled={failure.pending}
            onClick={() =>
              void failure
                .run(() => onDeleteProject(group.projectId))
                .then((ok) => {
                  if (ok) setConfirming(false);
                })
            }
          >
            {failure.pending ? "Deleting…" : "Delete project"}
          </button>
        </div>
        <InlineError testId={`project-delete-error-${group.projectId}`} message={failure.error} className="mt-2" />
      </div>
    );
  }
  return (
    <button
      type="button"
      data-testid={`project-delete-${group.projectId}`}
      className="mt-3 rounded-md border border-destructive px-3 py-1 text-sm font-medium text-destructive"
      onClick={() => setConfirming(true)}
    >
      Delete project
    </button>
  );
}
