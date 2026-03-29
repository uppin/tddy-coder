/**
 * Backend (agent) dropdown helpers for ConnectionScreen — driven by `ListAgents` (PRD).
 * Green phase: wire these into `ConnectionScreen` and align with RPC payloads.
 */

export type AgentOption = { value: string; label: string };

export type RpcAgent = { id: string; label: string };

/** Maps `ListAgents` entries to `<option>` value/label pairs. */
export function buildAgentSelectOptionsFromRpc(agents: RpcAgent[]): AgentOption[] {
  const out = agents.map((a) => ({ value: a.id, label: a.label }));
  console.debug("[agentOptions] buildAgentSelectOptionsFromRpc", {
    count: out.length,
    ids: out.map((o) => o.value),
  });
  return out;
}

/**
 * Default agent id: preserve `previousAgentId` when still in the list; otherwise first agent;
 * empty list → empty string.
 */
export function coalesceBackendAgentSelection(
  agents: AgentOption[],
  previousAgentId: string | undefined,
): string {
  if (agents.length === 0) {
    console.debug("[agentOptions] coalesceBackendAgentSelection: no agents → empty string");
    return "";
  }
  const previousOk =
    previousAgentId !== undefined &&
    previousAgentId !== "" &&
    agents.some((o) => o.value === previousAgentId);
  const chosen = previousOk ? previousAgentId! : agents[0].value;
  console.debug("[agentOptions] coalesceBackendAgentSelection", {
    previousAgentId,
    previousOk,
    chosen,
  });
  return chosen;
}
