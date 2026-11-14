import { useEffect } from "react";
import { AGENT_CLI_LAUNCH, type sessionTypeOptions } from "./cliHarnessOptions";
import { agentForSessionType, hasCapability } from "./codingAgentMetadata";
import type { SelectableAgent } from "./selectableAgentOptions";

/**
 * The main variant of the form opens on the one type that can run main: the first main-capable
 * agent the project's daemon serves. Does nothing on the ordinary form (`isMainVariant` false).
 */
export function useOpenOnMainSessionType({
  isMainVariant,
  typeOptions,
  offeredAgents,
  daemonInstanceId,
  connectedInstanceId,
  sessionType,
  setSessionType,
}: {
  isMainVariant: boolean;
  typeOptions: ReturnType<typeof sessionTypeOptions>;
  offeredAgents: readonly SelectableAgent[];
  daemonInstanceId: string;
  connectedInstanceId: string;
  sessionType: string;
  setSessionType: (sessionType: string) => void;
}): void {
  const mainSessionType = typeOptions.find(
    (option) =>
      option.launch === AGENT_CLI_LAUNCH &&
      hasCapability(
        agentForSessionType(offeredAgents, daemonInstanceId, connectedInstanceId, option.id),
        "project_home_main",
      ),
  )?.id;
  useEffect(() => {
    if (isMainVariant && mainSessionType && sessionType !== mainSessionType) {
      setSessionType(mainSessionType);
    }
  }, [isMainVariant, mainSessionType, sessionType, setSessionType]);
}
