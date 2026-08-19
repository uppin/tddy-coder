/**
 * Cross-daemon fan-out for `ConnectionService.ListAgents` — the agents a tool session can be started
 * *as*, gathered from the whole fleet.
 *
 * `ListAgents` carries no routing field, so a daemon answers with its own config allowlist plus its
 * own registry assistants and never forwards to its peers. A form that asks one daemon therefore
 * cannot offer an assistant created on another host — not merely hard to find, absent — which is the
 * bug this exists to fix.
 *
 * The fan-out is assembled the same way `useAvailableAgents` assembles the specialized-agent picker's:
 * one client per common-room daemon, peers addressed at their `daemon-{instanceId}` RPC identity over
 * the shared common-room connection, each host read independently so **one unreachable host costs one
 * error row, never the select**.
 *
 * `AgentInfo` is `{ id, label }` — nothing on the wire says which host answered — so the host is
 * stamped on here, from the instance id the read was addressed to. That attribution is exact because
 * the caller chose the destination, and it needs no proto or daemon change. It stops being exact only
 * if a daemon ever forwards `ListAgents` to its peers, at which point an answer no longer speaks for
 * the responder alone.
 *
 * Feature: docs/ft/web/1-WIP/PRD-2026-08-19-session-agent-host-fan-out.md
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ConnectError, createClient, type Client } from "@connectrpc/connect";
import { ConnectionService } from "../../gen/connection_pb";
import { daemonRpcIdentity } from "../../lib/participantRole";
import { useSelectedDaemon } from "../../rpc/selectedDaemon";
import { useLiveKitTransportFactory } from "../../rpc/transportProvider";
import { selectableAgentValue, type SelectableAgent } from "./selectableAgentOptions";
// The failure row is the same claim the specialized-agent fan-out makes — a named host, and why it
// could not be asked — so it is the same type, not a second one of identical shape.
import type { AgentHostFailure } from "./useAvailableAgents";

type ConnectionClient = Client<typeof ConnectionService>;

export interface SelectableAgents {
  /** Every host's agents, in host order (home first), de-duplicated by `id@daemonInstanceId`. */
  readonly agents: SelectableAgent[];
  readonly failures: AgentHostFailure[];
}

/** One host's answer: the agents it offers, or why it could not say. */
interface HostAnswer {
  readonly agents: SelectableAgent[];
  /**
   * Non-empty when the read failed. Held apart from an empty `agents` because the two claims are
   * different — "this host offers no agents" versus "this host did not answer" — and only the first
   * one may be rendered as an absence.
   */
  readonly error: string;
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
 * the create-session form is handed the selected daemon's client. `homeInstanceId` names the host
 * behind it, both to stamp its answers and so its failure gets an error row addressed to a host an
 * operator can go and look at.
 */
export function useSelectableAgents(
  homeClient: ConnectionClient,
  homeInstanceId: string,
): SelectableAgents {
  const { room, daemons } = useSelectedDaemon();
  const liveKitFactory = useLiveKitTransportFactory();
  const [answers, setAnswers] = useState<ReadonlyMap<string, HostAnswer>>(new Map());

  // A read that lands after the form is gone has nobody to render it, and writing it would be a
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
        const response = await client.listAgents({}, { signal });
        put({
          agents: response.agents.map((info) => ({
            id: info.id,
            label: info.label,
            daemonInstanceId: instanceId,
          })),
          error: "",
        });
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
  const peerIds = useMemo(() => peerIdsKey.split("\n").filter((id) => id !== ""), [peerIdsKey]);

  useEffect(() => {
    const reads = new AbortController();
    void readHost(homeInstanceId, homeClient, reads.signal);
    for (const instanceId of peerIds) {
      // Every peer is addressed over the shared common-room connection; without a room there is no
      // way to reach it, and saying so beats a select that quietly omits that host.
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
    const agents: SelectableAgent[] = [];
    const failures: AgentHostFailure[] = [];
    // `id@host` is unique across hosts by construction, so a repeat means one host was reached twice
    // (its own client and its common-room identity). One option per pair, first answer wins —
    // otherwise the select would offer two rows carrying the same value.
    const seen = new Set<string>();
    for (const instanceId of [homeInstanceId, ...peerIds]) {
      const answer = answers.get(instanceId);
      if (!answer) continue;
      if (answer.error !== "") {
        failures.push({ daemonInstanceId: instanceId, message: answer.error });
        continue;
      }
      for (const agent of answer.agents) {
        // Qualified whatever the select ends up rendering: the key identifies the pair, and the
        // format is the option value's so the two cannot drift.
        const key = selectableAgentValue(agent, true);
        if (seen.has(key)) continue;
        seen.add(key);
        agents.push(agent);
      }
    }
    return { agents, failures };
  }, [answers, homeInstanceId, peerIds]);
}
