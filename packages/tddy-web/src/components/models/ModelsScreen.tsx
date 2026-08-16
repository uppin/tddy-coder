import { useState } from "react";
import type { AssignableTool, ProviderKind } from "../../gen/models_pb";
import {
  modelRowKey,
  type AssistantRow,
  type DaemonFailure,
  type ModelRow,
  type ProviderRow,
  type RegistryReadStatus,
} from "../../utils/mergeRegistryEntries";
import { AssistantsPanel } from "./AssistantsPanel";
import { CreateAssistantDialog } from "./CreateAssistantDialog";
import { ModelsTable } from "./ModelsTable";
import { ProvidersPanel } from "./ProvidersPanel";

/**
 * Presentational Models & Agents screen: providers, the merged model catalog, and the assistants
 * composed from it. All RPC wiring lives in the container (`ModelsAppPage`); this component is pure
 * props plus the local UI state of which model's create-assistant dialog is open.
 */
export interface ModelsScreenProps {
  providers: ProviderRow[];
  models: ModelRow[];
  assistants: AssistantRow[];
  /** Daemons whose registry could not be read — one error row each, never an empty page. */
  failures: DaemonFailure[];
  /** Enumeration errors by `providerRowKey`, shared by the provider rows and the stale marking. */
  providerErrors: ReadonlyMap<string, string>;
  modelErrors: ReadonlyMap<string, string>;
  /** Why the catalog is empty, when it is. */
  status: RegistryReadStatus;
  /** The exec catalog the given daemon advertises. */
  toolsFor: (daemonInstanceId: string) => readonly AssignableTool[];
  onAddProvider: (input: {
    kind: ProviderKind;
    label: string;
    baseUrl: string;
    apiKey: string;
  }) => Promise<string>;
  onRefreshProvider: (provider: ProviderRow) => void;
  onLoadModel: (model: ModelRow) => void;
  onUnloadModel: (model: ModelRow) => void;
  onOpenChat: (model: ModelRow) => void;
  onCreateAssistant: (input: {
    model: ModelRow;
    name: string;
    label: string;
    systemPrompt: string;
    tools: string[];
  }) => Promise<string>;
}

export function ModelsScreen({
  providers,
  models,
  assistants,
  failures,
  providerErrors,
  modelErrors,
  status,
  toolsFor,
  onAddProvider,
  onRefreshProvider,
  onLoadModel,
  onUnloadModel,
  onOpenChat,
  onCreateAssistant,
}: ModelsScreenProps) {
  const [assistantSource, setAssistantSource] = useState<ModelRow | null>(null);

  return (
    <div data-testid="models-screen">
      <ProvidersPanel
        providers={providers}
        providerErrors={providerErrors}
        onAddProvider={onAddProvider}
        onRefreshProvider={onRefreshProvider}
      />

      <ModelsTable
        models={models}
        failures={failures}
        modelErrors={modelErrors}
        providerErrors={providerErrors}
        status={status}
        onLoadModel={onLoadModel}
        onUnloadModel={onUnloadModel}
        onOpenChat={onOpenChat}
        onCreateAssistant={setAssistantSource}
      />

      <AssistantsPanel assistants={assistants} />

      {assistantSource ? (
        <CreateAssistantDialog
          key={modelRowKey(assistantSource)}
          model={assistantSource}
          tools={toolsFor(assistantSource.daemonInstanceId)}
          onSubmit={(input) => onCreateAssistant({ model: assistantSource, ...input })}
          onClose={() => setAssistantSource(null)}
        />
      ) : null}
    </div>
  );
}
