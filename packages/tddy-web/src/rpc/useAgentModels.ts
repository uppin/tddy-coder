import { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { ConnectionService, ModelInfo } from "../gen/connection_pb";

type ConnectionClient = Client<typeof ConnectionService>;

/**
 * State of an on-demand model probe for one agent. The model list for a backend is enumerated from
 * the underlying agent command (cursor `--list-models`, ACP `available_models`) or a curated list,
 * so it is fetched lazily per selected agent rather than up front. See
 * docs/ft/web/tool-session-model-selection.md.
 */
export interface AgentModelsState {
  models: ModelInfo[];
  defaultModel: string;
  loading: boolean;
  /** Human-readable probe error, or `null`. A failed probe is surfaced, never masked. */
  error: string | null;
}

const EMPTY: AgentModelsState = { models: [], defaultModel: "", loading: false, error: null };

/**
 * Fetch the models a backend supports via `ListAgentModels`, re-fetching whenever `agent` changes.
 * An empty `agent` yields an idle (empty) state without a request.
 */
export function useAgentModels(
  client: ConnectionClient,
  sessionToken: string,
  agent: string,
  daemonInstanceId: string,
): AgentModelsState {
  const [state, setState] = useState<AgentModelsState>(EMPTY);

  useEffect(() => {
    if (!agent) {
      setState(EMPTY);
      return;
    }
    let cancelled = false;
    setState({ models: [], defaultModel: "", loading: true, error: null });
    client
      .listAgentModels({ sessionToken, agent, daemonInstanceId })
      .then((resp) => {
        if (cancelled) return;
        setState({
          models: resp.models as ModelInfo[],
          defaultModel: resp.defaultModel,
          loading: false,
          error: null,
        });
      })
      .catch((err) => {
        if (cancelled) return;
        const message = err instanceof Error ? err.message : String(err);
        setState({ models: [], defaultModel: "", loading: false, error: message });
      });
    return () => {
      cancelled = true;
    };
  }, [client, sessionToken, agent, daemonInstanceId]);

  return state;
}
