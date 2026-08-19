/**
 * Cross-daemon fan-out for `ConnectionService.ListAgents` — the agents a tool session can be started
 * *as*, gathered from the whole fleet.
 *
 * `ListAgents` carries no routing field, so a daemon answers with its own config allowlist plus its
 * own registry assistants and never forwards to its peers. A form that asks one daemon therefore
 * cannot offer an assistant created on another host — not merely hard to find, absent — which is the
 * bug this exists to fix.
 *
 * `AgentInfo` is `{ id, label }` — nothing on the wire says which host answered — so the host is
 * stamped on here, from the instance id the read was addressed to. That attribution is exact because
 * the caller chose the destination, and it needs no proto or daemon change. It stops being exact only
 * if a daemon ever forwards `ListAgents` to its peers, at which point an answer no longer speaks for
 * the responder alone.
 *
 * The addressing, the per-host isolation and the de-duplication are `useHostFanOut`'s; what is here
 * is the `ListAgents` call and what its answers mean.
 *
 * Feature: docs/ft/web/session-agent-catalog-fan-out.md
 */

import { createClient, type Client } from "@connectrpc/connect";
import { ConnectionService } from "../../gen/connection_pb";
import { useHostFanOut, type HostReadFailure, type HostReader } from "../../rpc/useHostFanOut";
import { selectableAgentValue, type SelectableAgent } from "./selectableAgentOptions";

type ConnectionClient = Client<typeof ConnectionService>;

export interface SelectableAgents {
  /** Every host's agents, in host order (home first), de-duplicated by `id@daemonInstanceId`. */
  readonly agents: SelectableAgent[];
  readonly failures: HostReadFailure[];
}

const AGENT_READER: HostReader<ConnectionClient, SelectableAgent> = {
  clientFor: (transport) => createClient(ConnectionService, transport),
  read: async (client, daemonInstanceId, signal) => {
    const response = await client.listAgents({}, { signal });
    return response.agents.map((info) => ({
      id: info.id,
      label: info.label,
      daemonInstanceId,
    }));
  },
  // `id@host` is unique across hosts by construction, and reusing the option value's format keeps the
  // rows the select offers and the rows this hook considers distinct from drifting apart.
  keyOf: (agent) => selectableAgentValue(agent, true),
};

/**
 * Read the agents offered by the daemon `homeClient` addresses and by every other daemon in the
 * common room. See {@link useHostFanOut} for what `homeClient` and `homeInstanceId` are.
 */
export function useSelectableAgents(
  homeClient: ConnectionClient,
  homeInstanceId: string,
): SelectableAgents {
  const { rows, failures } = useHostFanOut(homeClient, homeInstanceId, AGENT_READER);
  return { agents: rows, failures };
}
