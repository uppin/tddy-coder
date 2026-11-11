/**
 * Cross-daemon fan-out for `CatalogService.ListAgents` — the agents a tool session can be started
 * *as*, and the session types each host declares, gathered from the whole fleet.
 *
 * `ListAgents` carries no routing field, so a daemon answers with its own config allowlist plus its
 * own registry assistants and never forwards to its peers. A form that asks one daemon therefore
 * cannot offer an assistant created on another host — not merely hard to find, absent — which is the
 * bug this exists to fix.
 *
 * Nothing on the wire says which host answered — so the host is
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
import { CatalogService } from "tddy-client-web/gen/catalog_pb";
import { useMemo } from "react";
import { selectableAgentValue, type SelectableAgent } from "./selectableAgentOptions";
import type { ServedSessionType } from "./cliHarnessOptions";
import { type HostReadFailure, type HostReader, useHostFanOut } from "tddy-client-web";

type CatalogClient = Client<typeof CatalogService>;

export interface SelectableAgents {
  /** Every host's agents, in host order (home first), de-duplicated by `id@daemonInstanceId`. */
  readonly agents: SelectableAgent[];
  /**
   * Every host's declared session types (`ListAgentsResponse.session_types`), in host order,
   * de-duplicated by `sessionType@daemonInstanceId`. Read in the same `ListAgents` call.
   */
  readonly sessionTypes: ServedSessionType[];
  readonly failures: HostReadFailure[];
}

/** One row of a host's `ListAgents` answer: an agent, or a session type the host declares. */
type CatalogRow =
  | { readonly kind: "agent"; readonly agent: SelectableAgent }
  | { readonly kind: "sessionType"; readonly sessionType: ServedSessionType };

const CATALOG_READER: HostReader<CatalogClient, CatalogRow> = {
  clientFor: (transport) => createClient(CatalogService, transport),
  read: async (client, daemonInstanceId, signal) => {
    const response = await client.listAgents({}, { signal });
    const agents: CatalogRow[] = response.agents.map((info) => ({
      kind: "agent",
      agent: {
        id: info.id,
        label: info.label,
        daemonInstanceId,
        routeProviders: info.routeProviders,
        sessionType: info.sessionType,
        capabilities: info.capabilities,
        runParams: info.runParams,
        permissionModes: info.permissionModes,
        defaultModel: info.defaultModel,
        placements: info.placements,
      },
    }));
    const sessionTypes: CatalogRow[] = response.sessionTypes.map((info) => ({
      kind: "sessionType",
      sessionType: {
        sessionType: info.sessionType,
        label: info.label,
        launch: info.launch,
        daemonInstanceId,
      },
    }));
    return [...agents, ...sessionTypes];
  },
  // `id@host` is unique across hosts by construction, and reusing the option value's format keeps the
  // rows the select offers and the rows this hook considers distinct from drifting apart.
  keyOf: (row) =>
    row.kind === "agent"
      ? `agent:${selectableAgentValue(row.agent, true)}`
      : `session-type:${row.sessionType.sessionType}@${row.sessionType.daemonInstanceId}`,
};

/**
 * Read the agents and declared session types offered by the daemon `homeClient` addresses and by
 * every other daemon in the common room. See {@link useHostFanOut} for what `homeClient` and
 * `homeInstanceId` are.
 */
export function useSelectableAgents(
  homeClient: CatalogClient,
  homeInstanceId: string,
): SelectableAgents {
  const { rows, failures } = useHostFanOut(homeClient, homeInstanceId, CATALOG_READER);
  return useMemo(
    () => ({
      agents: rows.flatMap((row) => (row.kind === "agent" ? [row.agent] : [])),
      sessionTypes: rows.flatMap((row) => (row.kind === "sessionType" ? [row.sessionType] : [])),
      failures,
    }),
    [rows, failures],
  );
}
