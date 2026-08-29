import { useState } from "react";
import type { ProviderKind } from "../../gen/models_pb";
import {
  assistantRowKey,
  modelRowKey,
  type AssistantRow,
  type DaemonFailure,
  type ModelRow,
  type ProviderRow,
  type RegistryReadStatus,
} from "../../utils/mergeRegistryEntries";
import { AssistantsPanel } from "./AssistantsPanel";
import { CreateAssistantDialog } from "./CreateAssistantDialog";
import { EditAssistantDialog } from "./EditAssistantDialog";
import { ModelsTable } from "./ModelsTable";
import { ProvidersPanel } from "./ProvidersPanel";
import type { ToolCatalog } from "./useModelRegistryFanOut";

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
  /** Errors from a write against a provider row, by `providerRowKey`. */
  providerActionErrors: ReadonlyMap<string, string>;
  /** Errors from a write against an assistant row, by `assistantRowKey`. */
  assistantErrors: ReadonlyMap<string, string>;
  modelErrors: ReadonlyMap<string, string>;
  /** Why the catalog is empty, when it is. */
  status: RegistryReadStatus;
  /** The exec catalog the given daemon advertises, or why it is not known. */
  toolsFor: (daemonInstanceId: string) => ToolCatalog;
  /** The daemon a newly added provider is created on. */
  addProviderTarget: string;
  onAddProvider: (input: {
    kind: ProviderKind;
    label: string;
    baseUrl: string;
    apiKey: string;
  }) => Promise<string>;
  onRefreshProvider: (provider: ProviderRow) => void;
  onDeleteProvider: (provider: ProviderRow) => void;
  onLoadModel: (model: ModelRow) => void;
  onUnloadModel: (model: ModelRow) => void;
  onOpenChat: (model: ModelRow) => void;
  /** Open a chat with an assistant — a model plus its system prompt and its tools. */
  onOpenAssistantChat: (assistant: AssistantRow) => void;
  onCreateAssistant: (input: {
    model: ModelRow;
    name: string;
    label: string;
    systemPrompt: string;
    tools: string[];
    replaces: string[];
  }) => Promise<string>;
  onUpdateAssistant: (input: {
    assistant: AssistantRow;
    label: string;
    systemPrompt: string;
    tools: string[];
    replaces: string[];
  }) => Promise<string>;
  onDeleteAssistant: (assistant: AssistantRow) => void;
}

export function ModelsScreen({
  providers,
  models,
  assistants,
  failures,
  providerErrors,
  providerActionErrors,
  assistantErrors,
  modelErrors,
  status,
  toolsFor,
  addProviderTarget,
  onAddProvider,
  onRefreshProvider,
  onDeleteProvider,
  onLoadModel,
  onUnloadModel,
  onOpenChat,
  onOpenAssistantChat,
  onCreateAssistant,
  onUpdateAssistant,
  onDeleteAssistant,
}: ModelsScreenProps) {
  const [assistantSource, setAssistantSource] = useState<ModelRow | null>(null);
  const [assistantUnderEdit, setAssistantUnderEdit] = useState<AssistantRow | null>(null);

  return (
    <div data-testid="models-screen">
      <ProvidersPanel
        providers={providers}
        providerErrors={providerErrors}
        providerActionErrors={providerActionErrors}
        failures={failures}
        status={status}
        addProviderTarget={addProviderTarget}
        onAddProvider={onAddProvider}
        onRefreshProvider={onRefreshProvider}
        onDeleteProvider={onDeleteProvider}
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

      <AssistantsPanel
        assistants={assistants}
        assistantErrors={assistantErrors}
        failures={failures}
        status={status}
        onOpenChat={onOpenAssistantChat}
        onEditAssistant={setAssistantUnderEdit}
        onDeleteAssistant={onDeleteAssistant}
      />

      {assistantSource ? (
        <CreateAssistantDialog
          key={modelRowKey(assistantSource)}
          model={assistantSource}
          toolCatalog={toolsFor(assistantSource.daemonInstanceId)}
          onSubmit={(input) => onCreateAssistant({ model: assistantSource, ...input })}
          onClose={() => setAssistantSource(null)}
        />
      ) : null}

      {assistantUnderEdit ? (
        <EditAssistantDialog
          key={assistantRowKey(assistantUnderEdit)}
          assistant={assistantUnderEdit}
          toolCatalog={toolsFor(assistantUnderEdit.daemonInstanceId)}
          onSubmit={(input) => onUpdateAssistant({ assistant: assistantUnderEdit, ...input })}
          onClose={() => setAssistantUnderEdit(null)}
        />
      ) : null}
    </div>
  );
}
