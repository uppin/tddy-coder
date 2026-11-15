import { useActionFailure } from "tddy-client-web";
import { InlineError } from "tddy-ui-web";
import type { ProjectGroup } from "./ProjectCard";

/** `deletedAt` (RFC 3339) in the reader's locale; the raw stamp when it does not parse. */
function deletedAtText(deletedAt: string): string {
  const at = new Date(deletedAt);
  return Number.isNaN(at.getTime()) ? deletedAt : at.toLocaleString();
}

/**
 * A soft-deleted project on the Projects screen (listed with "Show deleted projects"): its name,
 * who deleted it and when, that its data is kept on disk, and **Restore** (`RestoreProject`, which
 * the daemon applies on every host that owns the project). A refused restore is shown on the card.
 *
 * Test ids: `project-deleted-<projectId>`, `project-restore-<projectId>`,
 * `project-restore-error-<projectId>`.
 */
export function DeletedProjectCard({
  group,
  onRestoreProject,
}: {
  group: ProjectGroup;
  /** Restore the project; rejects with the daemon's reason. Absent: no Restore is offered. */
  onRestoreProject?: (projectId: string) => Promise<void>;
}) {
  const restore = useActionFailure();

  return (
    <div
      data-testid={`project-deleted-${group.projectId}`}
      className="rounded-md border border-dashed border-border p-3 text-sm text-muted-foreground"
    >
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="font-medium text-foreground">{group.name}</div>
          <div>
            Deleted{group.deletedBy ? ` by ${group.deletedBy}` : ""}{" "}
            <time dateTime={group.deletedAt}>{deletedAtText(group.deletedAt)}</time>
          </div>
          <div>Its main checkout, worktrees, sessions and stacks are kept on disk.</div>
        </div>
        {onRestoreProject ? (
          <button
            type="button"
            data-testid={`project-restore-${group.projectId}`}
            className="rounded-md border border-border px-3 py-1 text-sm font-medium text-foreground"
            disabled={restore.pending}
            onClick={() => void restore.run(() => onRestoreProject(group.projectId))}
          >
            {restore.pending ? "Restoring…" : "Restore"}
          </button>
        ) : null}
      </div>
      <InlineError testId={`project-restore-error-${group.projectId}`} message={restore.error} className="mt-2" />
    </div>
  );
}
