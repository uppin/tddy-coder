import type { Client } from "@connectrpc/connect";
import type { CatalogService } from "../../gen/catalog_pb";
import type { DaemonHost } from "../../lib/participantRole";
import type { HostReadFailure } from "../../rpc/useHostFanOut";
import { useSelectableAgents } from "./useSelectableAgents";
import {
  agentForHost,
  hostRunningSession,
  selectableAgentValue,
  type SelectableAgent,
} from "./selectableAgentOptions";

export interface CreateSessionAgentSelectionInput {
  catalogClient: Client<typeof CatalogService>;
  /** The daemon the browser's RPC reaches — the fan-out's home host. */
  connectedInstanceId: string;
  daemons: readonly DaemonHost[];
  /** The host the form will ask to run the session, exactly as the form holds it. */
  daemonInstanceId: string;
  /** The *name* of the agent the operator last picked, kept across host changes. */
  agent: string;
}

export interface CreateSessionAgentSelection {
  /** The agents this form may offer. */
  offeredAgents: readonly SelectableAgent[];
  /** The hosts whose silence the form may report. */
  offeredHostFailures: readonly HostReadFailure[];
  /** Whether there is a host to name at all. */
  hostsAdvertised: boolean;
  /** The host whose agents the session can actually be started as. */
  agentHostInstanceId: string;
  selectedAgentId: string;
  /** The agent select's value for the selected agent; `""` when there is none. */
  selectedAgentValue: string;
}

/**
 * Which agent the Start-Session form's tool session will start as, derived from the fleet's
 * catalog, the host taking the session and the name the operator picked.
 */
export function useCreateSessionAgentSelection({
  catalogClient,
  connectedInstanceId,
  daemons,
  daemonInstanceId,
  agent,
}: CreateSessionAgentSelectionInput): CreateSessionAgentSelection {
  // Every common-room daemon's agents — the thing a tool session is started *as*. `ListAgents`
  // answers for the responding daemon only, so without this fan-out an assistant created on another
  // host is absent from the form rather than merely hard to find.
  const selectableAgents = useSelectableAgents(catalogClient, connectedInstanceId);

  // Whether there is a host to name at all. The same condition the Host select is rendered on: with
  // no common room there is one host, so nothing to disambiguate and nothing to caption.
  const hostsAdvertised = daemons.length > 0;

  /**
   * The host whose agents the session can actually be started as — the host the form will ask for, in
   * the spelling the fan-out stamps its rows with. See `hostRunningSession` for why an empty
   * `daemon_instance_id` names a host rather than lacking one. The request is unaffected: it keeps
   * sending `daemonInstanceId` exactly as the form holds it.
   */
  const agentHostInstanceId = hostRunningSession(daemonInstanceId, connectedInstanceId);

  const offeredAgents = selectableAgents.agents;

  /**
   * The agent the session will actually start as: the picked name, as the host taking the session
   * offers it. Derived from `(agent, agentHostInstanceId)` rather than stored beside them, so neither
   * the control nor the request can carry a pair the host does not have — that is what makes a Host
   * change re-point the selection (same name on the new host, else that host's first, else none) and
   * what makes the opening selection the home host's first agent without any effect correcting state
   * after the fact.
   */
  const selectedAgent = agentForHost(offeredAgents, agentHostInstanceId, agent);

  return {
    offeredAgents,
    offeredHostFailures: selectableAgents.failures,
    hostsAdvertised,
    agentHostInstanceId,
    selectedAgentId: selectedAgent?.id ?? "",
    selectedAgentValue:
      selectedAgent === null ? "" : selectableAgentValue(selectedAgent, hostsAdvertised),
  };
}
