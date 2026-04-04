import type { AgentInfo, EligibleDaemonEntry, ToolInfo } from "../../gen/connection_pb";
import {
  buildAgentSelectOptionsFromRpc,
  coalesceBackendAgentSelection,
} from "./agentOptions";

export type ProjectSessionForm = {
  toolPath: string;
  agent: string;
  recipe: string;
  debugLogging: boolean;
  daemonInstanceId: string;
};

export function defaultProjectSessionForm(
  tools: ToolInfo[],
  agents: AgentInfo[],
  daemons: EligibleDaemonEntry[],
): ProjectSessionForm {
  const localDaemon = daemons.find((d) => d.isLocal);
  const agentOptions = buildAgentSelectOptionsFromRpc(
    agents.map((a) => ({ id: a.id, label: a.label })),
  );
  return {
    toolPath: tools[0]?.path ?? "",
    agent: coalesceBackendAgentSelection(agentOptions, undefined),
    recipe: "tdd",
    debugLogging: false,
    daemonInstanceId: localDaemon?.instanceId ?? daemons[0]?.instanceId ?? "",
  };
}
