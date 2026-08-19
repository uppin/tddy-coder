/**
 * Cross-daemon fan-out for `ConnectionService.ListSubagents` — the agents a picker can offer.
 *
 * A daemon answers `ListSubagents` for its **own** defs only; it never forwards to its peers (the
 * request carries no routing field at all). So a picker that asks one daemon can only ever offer
 * one host's agents, which is exactly what the qualified `agent_id` exists to fix: two hosts
 * routinely offer a def called "explorer", and only `explorer@<host>` says which one.
 *
 * The addressing, the per-host isolation and the de-duplication are `useHostFanOut`'s; what is here
 * is the `ListSubagents` call and what its answers mean.
 *
 * Feature: docs/ft/daemon/session-agent-roster.md § Web UI (AC48, AC49).
 */

import { createClient, type Client } from "@connectrpc/connect";
import { ConnectionService, type SubagentInfo } from "../../gen/connection_pb";
import { useHostFanOut, type HostReadFailure, type HostReader } from "../../rpc/useHostFanOut";

type ConnectionClient = Client<typeof ConnectionService>;

/** One agent a host offers, as a picker renders and submits it. */
export interface AvailableAgent {
  /** `name@daemon_instance_id`, minted by the serving daemon — the value a picker submits. */
  readonly agentId: string;
  readonly name: string;
  readonly label: string;
  readonly model: string;
  readonly daemonInstanceId: string;
  /** Exec tools the main agent loses while this agent is attached. */
  readonly replaces: readonly string[];
}

export interface AvailableAgents {
  /** Every host's agents, in host order, de-duplicated by qualified id. */
  readonly agents: AvailableAgent[];
  readonly failures: HostReadFailure[];
}

function availableAgentOf(info: SubagentInfo): AvailableAgent {
  return {
    agentId: info.agentId,
    name: info.name,
    label: info.label,
    model: info.model,
    daemonInstanceId: info.daemonInstanceId,
    replaces: info.replaces,
  };
}

const SUBAGENT_READER: HostReader<ConnectionClient, AvailableAgent> = {
  clientFor: (transport) => createClient(ConnectionService, transport),
  read: async (client, _daemonInstanceId, signal) => {
    // The host is on the wire here: the serving daemon mints the qualified `agent_id` and stamps
    // `daemon_instance_id` on every def it answers with.
    const response = await client.listSubagents({}, { signal });
    return response.subagents.map(availableAgentOf);
  },
  keyOf: (agent) => agent.agentId,
};

/**
 * Read the agents offered by the daemon `homeClient` addresses and by every other daemon in the
 * common room. See {@link useHostFanOut} for what `homeClient` and `homeInstanceId` are.
 */
export function useAvailableAgents(
  homeClient: ConnectionClient,
  homeInstanceId: string,
): AvailableAgents {
  const { rows, failures } = useHostFanOut(homeClient, homeInstanceId, SUBAGENT_READER);
  return { agents: rows, failures };
}
