import { useEffect, useState } from "react";
import { useAgentModels } from "./useAgentModels";
import {
  useProviderAccountModels,
  useProviderDefault,
  useProviderRoutes,
} from "./useProviderRoutes";
import { BUILT_IN_PROVIDER } from "./CreateSessionProviderField";
import type { CreateSessionPaneProps } from "./createSessionPaneProps";
import type { SessionType } from "./createSessionRequest";
import { TDDY_CODER_LAUNCH, type SessionLaunch } from "./cliHarnessOptions";

export interface UseCreateSessionModelArgs {
  catalogClient: CreateSessionPaneProps["catalogClient"];
  accountsClient: CreateSessionPaneProps["accountsClient"];
  sessionToken: string;
  sessionType: SessionType;
  /** How a session of `sessionType` is launched, as its daemon declared it. */
  sessionLaunch: SessionLaunch | undefined;
  /** The agent a tool session starts as (empty when none is selectable). */
  selectedAgentId: string;
  daemonInstanceId: string;
  /**
   * The provider kinds the selected tool-session agent's spec says a route can point it at. Empty:
   * no route is asked for. `undefined`: the catalog did not say, so the daemon is asked.
   */
  selectedAgentRouteProviders?: readonly string[];
  /**
   * The provider kinds a route can point the CLI session's agent at, from its served metadata.
   * Empty: it always runs on its own vendor, so no route is asked for. `undefined`: not served.
   */
  cliRouteProviders?: readonly string[];
  /** The caller's pre-filled model, if any. */
  initialModel: string | undefined;
}

/**
 * The session's model and the provider it runs on: the provider choice and the routes the daemon
 * offers for the harness, the catalog the model is picked from (the vendor's, or the chosen
 * account's own), and the two reset effects that keep both in step with the session type and the
 * catalog. Lifted out of `CreateSessionPane.tsx` with its state; the effects keep their order.
 */
export function useCreateSessionModel({
  catalogClient,
  accountsClient,
  sessionToken,
  sessionType,
  sessionLaunch,
  selectedAgentId,
  daemonInstanceId,
  selectedAgentRouteProviders,
  cliRouteProviders,
  initialModel,
}: UseCreateSessionModelArgs) {
  // A tddy-coder session runs the agent the form picked; any other session type is its own harness.
  const runsThePickedAgent = sessionLaunch === TDDY_CODER_LAUNCH;
  const [model, setModel] = useState(initialModel ?? "");
  // Where the session's harness runs. `null` until the operator chooses: an untouched form names no
  // provider, so a default the daemon holds for the harness may still decide; choosing "built in"
  // is an explicit choice that overrides it.
  const [providerChoice, setProviderChoice] = useState<string | null>(null);

  // The model catalog is enumerated per selected backend: the chosen agent for tool sessions, and
  // for a CLI session type the pseudo-agent named after it ("claude-cli", "cursor-cli", …).
  const modelAgentKey = runsThePickedAgent ? selectedAgentId : sessionType;
  const vendorModels = useAgentModels(catalogClient, sessionToken, modelAgentKey, daemonInstanceId);

  // The provider field is offered for the harness the session runs: a CLI session type names its
  // harness, and a tool session runs the agent it picked. The catalog says which agents a route
  // can point elsewhere (`AgentInfo.routeProviders`): an agent with none — a specialized
  // definition, or one that always runs on its own vendor — names no harness and no provider is
  // looked up or offered for it. The daemon says which accounts that harness can run on and why
  // not; the form keeps no matrix of its own.
  const providerHarness = providerHarnessFor(
    sessionType,
    runsThePickedAgent,
    selectedAgentId,
    runsThePickedAgent ? selectedAgentRouteProviders : cliRouteProviders,
  );
  const providerRoutes = useProviderRoutes(accountsClient, sessionToken, providerHarness);
  // A provider choice belongs to the harness it was made for: switching session type or agent
  // clears it.
  useEffect(() => {
    setProviderChoice(null);
  }, [providerHarness]);
  // The person's own default for this harness is where the form starts; they can still change it.
  const providerDefault = useProviderDefault(accountsClient, sessionToken, providerHarness);
  useEffect(() => {
    const usable = providerRoutes.choices.some(
      (choice) => choice.accountId === providerDefault && choice.supported,
    );
    if (providerDefault !== null && usable) {
      setProviderChoice((chosen) => chosen ?? providerDefault);
    }
  }, [providerDefault, providerRoutes.choices]);
  const providerAccountId =
    providerChoice !== null && providerChoice !== BUILT_IN_PROVIDER ? providerChoice : "";
  // An account serves its own models; the vendor's catalog does not apply to it.
  const providerModels = useProviderAccountModels(accountsClient, sessionToken, providerAccountId);
  const agentModels = providerAccountId !== "" ? providerModels : vendorModels;

  // Reset the model selection to the backend's advertised default whenever the catalog changes
  // (agent switch, session-type switch, provider switch). Empty while loading or on a failed probe.
  useEffect(() => {
    setModel(agentModels.defaultModel);
  }, [agentModels.defaultModel]);

  return {
    model,
    setModel,
    providerChoice,
    setProviderChoice,
    providerHarness,
    providerRoutes,
    providerAccountId,
    agentModels,
  };
}

/**
 * The harness whose provider accounts the form offers: the agent a tool session picked, or the
 * session type of a CLI session — and none for an agent whose served metadata says no route can
 * point it elsewhere (`routeProviders` empty), since it runs only on its own vendor.
 */
function providerHarnessFor(
  sessionType: SessionType,
  runsThePickedAgent: boolean,
  selectedAgentId: string,
  routeProviders: readonly string[] | undefined,
): string | null {
  if (routeProviders !== undefined && routeProviders.length === 0) return null;
  if (runsThePickedAgent) return selectedAgentId || null;
  return sessionType;
}
