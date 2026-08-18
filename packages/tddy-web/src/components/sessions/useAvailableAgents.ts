/**
 * Cross-daemon fan-out for `ConnectionService.ListSubagents` — the agents a picker can offer.
 *
 * A daemon answers `ListSubagents` for its **own** defs only; it never forwards to its peers (the
 * request carries no routing field at all). So a picker that asks one daemon can only ever offer
 * one host's agents, which is exactly what the qualified `agent_id` exists to fix: two hosts
 * routinely offer a def called "explorer", and only `explorer@<host>` says which one.
 *
 * The fan-out is therefore assembled here, the same way `components/models/useModelRegistryFanOut`
 * assembles the fleet's model registry: one client per common-room daemon, addressed at its
 * `daemon-{instanceId}` RPC identity over the shared common-room connection, each host read
 * independently so **one unreachable host costs one error row, never the picker**.
 *
 * Feature: docs/ft/daemon/session-agent-roster.md § Web UI (AC48, AC49).
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ConnectError, createClient, type Client } from "@connectrpc/connect";
import { ConnectionService, type SubagentInfo } from "../../gen/connection_pb";
import { daemonRpcIdentity } from "../../lib/participantRole";
import { useSelectedDaemon } from "../../rpc/selectedDaemon";
import { useLiveKitTransportFactory } from "../../rpc/transportProvider";

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

/** A host that could not be asked, or refused to answer. Rendered as one row, never swallowed. */
export interface AgentHostFailure {
  readonly daemonInstanceId: string;
  readonly message: string;
}

export interface AvailableAgents {
  /** Every host's agents, in host order, de-duplicated by qualified id. */
  readonly agents: AvailableAgent[];
  readonly failures: AgentHostFailure[];
}

/** One host's answer: the agents it offers, or why it could not say. */
interface HostAnswer {
  readonly agents: AvailableAgent[];
  /**
   * Non-empty when the read failed. Held apart from an empty `agents` because the two claims are
   * different — "this host offers no agents" versus "this host did not answer" — and only the first
   * one may be rendered as an absence.
   */
  readonly error: string;
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

/** What a caller is told when the common room holds no connection to the host it addressed. */
function noConnectionTo(daemonInstanceId: string): string {
  return `no connection to daemon ${daemonInstanceId}`;
}

/**
 * Read the agents offered by the daemon `homeClient` addresses and by every other daemon in the
 * common room.
 *
 * `homeClient` is passed in rather than derived because the surface that owns it already has one:
 * the create-session form is handed the selected daemon's client, and the roster pane talks over the
 * shared transport to the daemon facilitating its session. `homeInstanceId` names the host behind
 * it, so its failure gets an error row addressed to a host an operator can go and look at.
 */
export function useAvailableAgents(
  homeClient: ConnectionClient,
  homeInstanceId: string,
): AvailableAgents {
  const { room, daemons } = useSelectedDaemon();
  const liveKitFactory = useLiveKitTransportFactory();
  const [answers, setAnswers] = useState<ReadonlyMap<string, HostAnswer>>(new Map());

  // A read that lands after the picker is gone has nobody to render it, and writing it would be a
  // state update against a dead component.
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const readHost = useCallback(
    async (instanceId: string, client: ConnectionClient, signal: AbortSignal): Promise<void> => {
      const put = (answer: HostAnswer) => {
        if (!mounted.current || signal.aborted) return;
        setAnswers((current) => new Map(current).set(instanceId, answer));
      };
      try {
        const response = await client.listSubagents({}, { signal });
        put({ agents: response.subagents.map(availableAgentOf), error: "" });
      } catch (err) {
        put({ agents: [], error: ConnectError.from(err).rawMessage });
      }
    },
    [],
  );

  // The daemon list is rebuilt on every common-room participant event, so its array identity changes
  // far more often than its contents; depending on the ids keeps the fan-out to one read per host
  // per actual change. `\n` cannot occur in a daemon instance id (they are LiveKit participant
  // identity segments).
  const peerIdsKey = daemons
    .map((d) => d.instanceId)
    .filter((id) => id !== "" && id !== homeInstanceId)
    .join("\n");
  const peerIds = useMemo(
    () => peerIdsKey.split("\n").filter((id) => id !== ""),
    [peerIdsKey],
  );

  useEffect(() => {
    const reads = new AbortController();
    void readHost(homeInstanceId, homeClient, reads.signal);
    for (const instanceId of peerIds) {
      // Every peer is addressed over the shared common-room connection; without a room there is no
      // way to reach it, and saying so beats offering a picker that quietly omits that host.
      if (!room) {
        setAnswers((current) =>
          new Map(current).set(instanceId, { agents: [], error: noConnectionTo(instanceId) }),
        );
        continue;
      }
      const transport = liveKitFactory(room, daemonRpcIdentity(instanceId));
      void readHost(instanceId, createClient(ConnectionService, transport), reads.signal);
    }
    // Unmounting — or moving on to a different host list — cancels the reads in flight, so no answer
    // to a question nobody is asking any more is waited for or written.
    return () => reads.abort();
  }, [homeClient, homeInstanceId, peerIds, room, liveKitFactory, readHost]);

  return useMemo(() => {
    const agents: AvailableAgent[] = [];
    const failures: AgentHostFailure[] = [];
    // Qualified ids are unique across hosts by construction, so a repeat means one host was reached
    // twice (its own client and its common-room identity). One row per id, first answer wins —
    // otherwise the picker would render two controls carrying the same id.
    const seen = new Set<string>();
    for (const instanceId of [homeInstanceId, ...peerIds]) {
      const answer = answers.get(instanceId);
      if (!answer) continue;
      if (answer.error !== "") {
        failures.push({ daemonInstanceId: instanceId, message: answer.error });
        continue;
      }
      for (const agent of answer.agents) {
        if (seen.has(agent.agentId)) continue;
        seen.add(agent.agentId);
        agents.push(agent);
      }
    }
    return { agents, failures };
  }, [answers, homeInstanceId, peerIds]);
}
