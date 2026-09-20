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
}

/**
 * What the claude-cli **Managed codebase** toggle opens: the recipe picker, the codebase host and
 * the SSH host. Presentational — every value stays in `CreateSessionPane`, which decides what each
 * placement withdraws.
 *
 * The specialized-agent picker and the Semantic index are **not** here. Neither is a property of
 * the orchestration this toggle turns on: an agent is placeable on any host and reads the codebase
 * through the session's own placement, and the index is built wherever the worktree is. Owning
 * them here meant choosing any other placement took the controls off the page — see
 * `docs/ft/daemon/amendments/PRD-2026-09-20-sandboxed-codebase-managed-workflow.md`.
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
    </div>
  );
}
