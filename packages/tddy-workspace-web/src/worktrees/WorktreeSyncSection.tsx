import React from "react";
import { WorktreeSyncStatus, type WorktreeCommitLine, type WorktreeSyncState } from "tddy-client-web/gen/worktree_pb";
import { Button } from "tddy-ui-web";

const STATUS_LABEL: Record<WorktreeSyncStatus, string> = {
  [WorktreeSyncStatus.UNSPECIFIED]: "Unknown",
  [WorktreeSyncStatus.IN_SYNC]: "In sync",
  [WorktreeSyncStatus.UNPUSHED]: "Unpushed",
  [WorktreeSyncStatus.UNPULLED]: "Unpulled",
  [WorktreeSyncStatus.DIVERGED]: "Diverged",
  [WorktreeSyncStatus.UNPUBLISHED]: "Unpublished",
};

function CommitLines({ lines, testId }: { lines: WorktreeCommitLine[]; testId: string }) {
  return (
    <ul className="flex flex-col gap-0.5">
      {lines.map((line) => (
        <li key={line.sha} data-testid={`${testId}-${line.sha}`}>
          <code className="text-foreground">{line.sha}</code> {line.subject}
        </li>
      ))}
    </ul>
  );
}

export interface WorktreeSyncSectionProps {
  state: WorktreeSyncState;
  acting: boolean;
  onPull: () => void;
  onPush: () => void;
}

/** The Worktree tab's sync sections: status against the upstream, its commits, dirty files, Pull/Push. */
export function WorktreeSyncSection({ state, acting, onPull, onPush }: WorktreeSyncSectionProps) {
  if (state.unavailableReason !== "") {
    return <p data-testid="session-worktree-sync-unavailable">{state.unavailableReason}</p>;
  }
  return (
    <div className="flex flex-col gap-2">
      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1">
        <dt>Sync</dt>
        <dd className="text-foreground" data-testid="session-worktree-sync-status">
          {STATUS_LABEL[state.status]}
          {state.upstream !== "" ? ` (${state.upstream})` : ""}
        </dd>
      </dl>
      {state.unpushed.length > 0 ? (
        <div>
          <div>Unpushed commits</div>
          <CommitLines lines={state.unpushed} testId="session-worktree-unpushed" />
        </div>
      ) : null}
      {state.unpulled.length > 0 ? (
        <div>
          <div>Unpulled commits</div>
          <CommitLines lines={state.unpulled} testId="session-worktree-unpulled" />
        </div>
      ) : null}
      {state.dirtyFiles.length > 0 ? (
        <div>
          <div>Dirty files</div>
          <ul className="flex flex-col gap-0.5">
            {state.dirtyFiles.map((file) => (
              <li key={file.path} data-testid={`session-worktree-dirty-file-${file.path}`}>
                <span className="text-foreground">{file.path}</span> {file.change}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
      {state.canPull || state.canPush ? (
        <div className="flex gap-2">
          {state.canPull ? (
            <Button
              type="button"
              size="xs"
              variant="outline"
              data-testid="session-worktree-pull"
              disabled={acting}
              onClick={onPull}
            >
              Pull
            </Button>
          ) : null}
          {state.canPush ? (
            <Button
              type="button"
              size="xs"
              variant="outline"
              data-testid="session-worktree-push"
              disabled={acting}
              onClick={onPush}
            >
              Push
            </Button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
