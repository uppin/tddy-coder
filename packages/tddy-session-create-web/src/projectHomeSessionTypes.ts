import { hasCapability } from "./codingAgentMetadata";
import { AGENT_CLI_LAUNCH, type SessionLaunch } from "./cliHarnessOptions";
import type { SelectableAgent } from "./selectableAgentOptions";

/**
 * Why a session of the launch kind `sessionLaunch` cannot be a project's **main**, or `null` when it
 * can. A form holds this beside its type picker: main is not a session that merely looks right, it
 * is the project's standing record (`docs/ft/daemon/projects.md` § *Every project has a main*), and
 * a type that cannot be one is refused by the daemon *by name* — the form saying so first keeps the
 * refusal off the wire.
 *
 * Main is a managed-workflow run on the project's own repository, and the agent must declare it can
 * stand as one: only a coding agent's CLI session type (`agent_cli`) can, and then its spec's
 * `project_home_main` capability is the whole test, read from the metadata its daemon served. The
 * bundle names no agent and no session type.
 *
 * `undefined` (no start attempted yet, or a type no daemon declared) answers as a workflow session
 * does: there is no agent to stand as main.
 */
export function whyMainCannotRun(
  sessionLaunch: SessionLaunch | undefined,
  agent?: SelectableAgent,
): string | null {
  if (sessionLaunch !== AGENT_CLI_LAUNCH) {
    return "A workflow session cannot be a project's main: main is a coding agent's own session on the project's repository.";
  }
  if (hasCapability(agent, "project_home_main")) return null;
  return "This agent's spec does not offer it as a project's main: main needs an agent that declares it can stand as one.";
}
