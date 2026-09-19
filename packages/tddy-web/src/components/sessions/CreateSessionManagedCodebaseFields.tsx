import type { ReactNode } from "react";
import type { DaemonHost } from "../../lib/participantRole";
import { CreateSessionSshConfigSelect } from "./CreateSessionSshConfigSelect";
import { inputClass, labelClass } from "./createSessionFormStyles";
import { WORKFLOW_RECIPES } from "./createSessionRecipes";
import type { SessionType } from "./createSessionRequest";

export interface CreateSessionManagedCodebaseFieldsProps {
  isSplitCodebase: boolean;
  recipe: string;
  setRecipe: (recipe: string) => void;
  canChooseCodebaseHost: boolean;
  codebaseDaemonInstanceId: string;
  setCodebaseDaemonInstanceId: (daemonInstanceId: string) => void;
  daemons: DaemonHost[];
  sessionType: SessionType;
  sshListDaemonId: string;
  sessionToken: string;
  sshConfigHost: string;
  setSshConfigHost: (sshConfigHost: string) => void;
  agentPickerSection: ReactNode;
  semanticIndex: boolean;
  setSemanticIndex: (semanticIndex: boolean) => void;
}

/**
 * What the claude-cli **Managed codebase** toggle opens: the recipe picker, the codebase host, the
 * SSH host, the specialized-agent picker and the semantic index. Presentational — every value stays
 * in `CreateSessionPane`, which decides what each placement withdraws.
 */
export function CreateSessionManagedCodebaseFields({
  isSplitCodebase,
  recipe,
  setRecipe,
  canChooseCodebaseHost,
  codebaseDaemonInstanceId,
  setCodebaseDaemonInstanceId,
  daemons,
  sessionType,
  sshListDaemonId,
  sessionToken,
  sshConfigHost,
  setSshConfigHost,
  agentPickerSection,
  semanticIndex,
  setSemanticIndex,
}: CreateSessionManagedCodebaseFieldsProps) {
  return (
    <div className="mt-2 space-y-3 pl-4">
      {/* A recipe's tooling runs against a repository on the daemon hosting the agent, and
          a split session has none — the daemon refuses the combination. Withdrawing the
          control is honest about that; leaving it visible would offer a choice whose only
          effect is to turn a valid placement into a refusal. */}
      {!isSplitCodebase && (
        <div>
          <label className={labelClass} htmlFor="create-session-recipe">
            Recipe
          </label>
          <select
            id="create-session-recipe"
            data-testid="create-session-recipe-select"
            className={inputClass}
            value={recipe}
            onChange={(e) => setRecipe(e.target.value)}
          >
            {WORKFLOW_RECIPES.map((r) => (
              <option key={r} value={r}>
                {r}
              </option>
            ))}
          </select>
        </div>
      )}
      {/* Codebase host — which daemon's filesystem holds the worktree. Offered only in the
          claude-cli copy of this block: only claude-cli can be *prevented* from touching a
          local filesystem (--allowedTools/--disallowedTools), so it is the only session
          type the daemon accepts a split placement for.
          See docs/ft/daemon/remote-managed-worktree.md. */}
      {canChooseCodebaseHost && (
        <div>
          <label className={labelClass} htmlFor="create-session-codebase-host">
            Codebase host
          </label>
          <select
            id="create-session-codebase-host"
            data-testid="create-session-codebase-host-select"
            className={inputClass}
            value={codebaseDaemonInstanceId}
            onChange={(e) => setCodebaseDaemonInstanceId(e.target.value)}
          >
            <option value="">Same as host</option>
            {daemons.map((d) => (
              <option key={d.instanceId} value={d.instanceId}>
                {d.label}
              </option>
            ))}
          </select>
        </div>
      )}
      {sessionType === "claude-cli" && (
        <CreateSessionSshConfigSelect
          key={sshListDaemonId}
          sessionToken={sessionToken}
          listDaemonInstanceId={sshListDaemonId}
          value={sshConfigHost}
          onChange={setSshConfigHost}
        />
      )}
      {/* No split guard: an agent is placeable on any host, and the placement only
          decides how it reads the codebase — an agent on the codebase host reads that
          worktree directly, one anywhere else reads a clone the session's worktree sync
          keeps current. So the picker offers the same roster either way. */}
      {agentPickerSection}
      {/* No split guard: the index is built wherever the worktree is, which on a split
          session is the codebase host. */}
      <div>
        <label className="flex items-center gap-2 text-sm text-muted-foreground">
          <input
            data-testid="create-session-semantic-index-toggle"
            type="checkbox"
            className="h-4 w-4 rounded border-input"
            checked={semanticIndex}
            onChange={(e) => setSemanticIndex(e.target.checked)}
          />
          Semantic index
        </label>
      </div>
    </div>
  );
}
