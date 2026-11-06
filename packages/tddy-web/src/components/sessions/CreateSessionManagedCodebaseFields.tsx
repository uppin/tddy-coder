import type { DaemonHost } from "../../lib/participantRole";
import { CreateSessionSshConfigSelect } from "./CreateSessionSshConfigSelect";
import { inputClass, labelClass } from "./createSessionFormStyles";
import { WORKFLOW_RECIPES } from "./createSessionRecipes";

export interface CreateSessionManagedCodebaseFieldsProps {
  isSplitCodebase: boolean;
  recipe: string;
  setRecipe: (recipe: string) => void;
  canChooseCodebaseHost: boolean;
  codebaseDaemonInstanceId: string;
  setCodebaseDaemonInstanceId: (daemonInstanceId: string) => void;
  daemons: DaemonHost[];
  /** The served agent declares `split_host`, so the SSH exec-catalog host is offered with it. */
  showsSshHost: boolean;
  sshListDaemonId: string;
  sessionToken: string;
  sshConfigHost: string;
  setSshConfigHost: (sshConfigHost: string) => void;
}

/**
 * What the **Managed codebase** toggle opens: the recipe picker, the codebase host and
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
  showsSshHost,
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
      {/* Codebase host — which daemon's filesystem holds the worktree. Offered only for an
          agent whose spec declares `split_host`: only it can be *prevented* from touching a
          local filesystem (its spec's tool-withdrawal), so it is the only kind the daemon
          accepts a split placement for.
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
      {showsSshHost && (
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
