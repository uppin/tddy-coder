import { hasCapability } from "./codingAgentMetadata";
import { TOOL_SESSION_TYPE } from "./cliHarnessOptions";
import type { SelectableAgent } from "./selectableAgentOptions";

/**
 * Why a session of `sessionType` cannot be a project's **main**, or `null` when it can. A form
 * holds this beside its type picker: main is not a session that merely looks right, it is the
 * project's standing record (`docs/ft/daemon/projects.md` § *Every project has a main*), and a
 * type that cannot be one is refused by the daemon *by name* — the form saying so first keeps
 * the refusal off the wire.
 *
 * Main is a managed-workflow run on the project's own repository, and the agent must declare
 * it can stand as one: its spec's `project_home_main` capability is the whole test, read from
 * the metadata its daemon served. The bundle names no agent.
 *
 * `undefined` (no start attempted yet) is a tool session's absence rather than a type that
 * cannot run main, so it answers the same as `tool`.
 */
export function whyMainCannotRun(
  sessionType: string | undefined,
  agent: SelectableAgent | undefined,
): string | null {
  const normalized = sessionType ?? TOOL_SESSION_TYPE;
  if (normalized === TOOL_SESSION_TYPE) {
    return "A workflow session cannot be a project's main: main is a coding agent's own session on the project's repository.";
  }
  if (hasCapability(agent, "project_home_main")) return null;
  return "This agent's spec does not offer it as a project's main: main needs an agent that declares it can stand as one.";
}
