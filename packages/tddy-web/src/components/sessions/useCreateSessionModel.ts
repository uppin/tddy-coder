import { useEffect, useState } from "react";
import { useAgentModels } from "../../rpc/useAgentModels";
import { useProviderAccountModels, useProviderRoutes } from "../../rpc/useProviderRoutes";
import { BUILT_IN_PROVIDER } from "./CreateSessionProviderField";
import type { CreateSessionPaneProps } from "./createSessionPaneProps";
import type { SessionType } from "./createSessionRequest";

export interface UseCreateSessionModelArgs {
  catalogClient: CreateSessionPaneProps["catalogClient"];
  accountsClient: CreateSessionPaneProps["accountsClient"];
  sessionToken: string;
  sessionType: SessionType;
  /** The agent a tool session starts as (empty when none is selectable). */
  selectedAgentId: string;
  daemonInstanceId: string;
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
  selectedAgentId,
  daemonInstanceId,
  initialModel,
}: UseCreateSessionModelArgs) {
  const [model, setModel] = useState(initialModel ?? "");
  // Where the session's harness runs. `null` until the operator chooses: an untouched form names no
  // provider, so a default the daemon holds for the harness may still decide; choosing "built in"
  // is an explicit choice that overrides it.
  const [providerChoice, setProviderChoice] = useState<string | null>(null);

  // The model catalog is enumerated per selected backend: the chosen agent for tool sessions, and
  // for a CLI session type the pseudo-agent named after it ("claude-cli", "cursor-cli", …).
  const modelAgentKey = sessionType === "tool" ? selectedAgentId : sessionType;
  const vendorModels = useAgentModels(catalogClient, sessionToken, modelAgentKey, daemonInstanceId);

  // The provider field is offered for the harness the session runs: a CLI session type names its
  // harness, and a tool session runs the agent it picked. The daemon says which accounts that
  // harness can run on and why not; the form keeps no matrix of its own.
  // TODO(tool-route): the catalog does not say which agents are specialized definitions, which are
  // not harnesses; for one of those the daemon's reason ("unknown harness") shows in this field.
  const providerHarness = sessionType === "tool" ? selectedAgentId || null : sessionType;
  const providerRoutes = useProviderRoutes(accountsClient, sessionToken, providerHarness);
  // A provider choice belongs to the harness it was made for: switching session type or agent
  // clears it.
  useEffect(() => {
    setProviderChoice(null);
  }, [providerHarness]);
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
