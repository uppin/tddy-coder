import type { SelectableAgent } from "./selectableAgentOptions";

/**
 * What the Create pane asks of the coding agent behind a session type, read from the
 * metadata the **target daemon** served for that agent (`CatalogService.ListAgents`):
 * its spec's capabilities, the params its launch argv binds, its permission modes and its
 * default model. The bundle hard-codes no agent: an agent the daemon does not serve is
 * not offered, and its fields follow what its spec declares.
 */

/** The agent behind `sessionType` on the daemon `sessionType` starts on, when served. */
export function agentForSessionType(
  agents: readonly SelectableAgent[],
  daemonInstanceId: string,
  connectedInstanceId: string,
  sessionType: string,
): SelectableAgent | undefined {
  const wanted = new Set(
    [daemonInstanceId, connectedInstanceId].filter((id) => id !== ""),
  );
  const onHost = (agent: SelectableAgent) =>
    wanted.size === 0 || wanted.has(agent.daemonInstanceId);
  return agents.find(
    (agent) => agent.sessionType === sessionType && onHost(agent),
  );
}

/** Whether the agent's launch argv binds `param` (`model`, `permission_mode`, …). */
export function bindsParam(
  agent: SelectableAgent | undefined,
  param: string,
): boolean {
  return agent?.runParams.includes(param) ?? false;
}

/** Whether the agent's spec declares `capability` (`managed_workflow`, `split_host`, …). */
export function hasCapability(
  agent: SelectableAgent | undefined,
  capability: string,
): boolean {
  return agent?.capabilities.includes(capability) ?? false;
}

/** Whether the agent's spec allows `placement` (`host`, `jail`). */
export function allowsPlacement(
  agent: SelectableAgent | undefined,
  placement: string,
): boolean {
  return agent?.placements.includes(placement) ?? false;
}
