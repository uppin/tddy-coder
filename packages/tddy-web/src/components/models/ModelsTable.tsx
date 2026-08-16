import { ModelLoadState } from "../../gen/models_pb";
import {
  modelRowKey,
  providerRowKey,
  registryEmptyStateText,
  type DaemonFailure,
  type ModelRow,
  type RegistryReadStatus,
} from "../../utils/mergeRegistryEntries";
import { unrecognisedEnumText } from "../../lib/enumSkew";
import { safeTestIdPart } from "../../lib/testId";

/**
 * The merged model catalog: every model every connected daemon's providers offer, one row each,
 * labelled by capability and by residency, offering only the actions that model's state permits.
 *
 * A daemon whose registry could not be read gets one error row of its own — the other daemons'
 * models still list, because a fleet is not down just because one host is
 * (docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md § AC12).
 */

/**
 * The `data-load-state` vocabulary, mirroring `models.ModelLoadState`. A value outside the enum this
 * build knows is reported as itself — collapsing it into one of the known words would let a daemon
 * newer than this tab read as ordinary data.
 */
export function loadStateName(state: ModelLoadState): string {
  switch (state) {
    case ModelLoadState.LOADED:
      return "loaded";
    case ModelLoadState.NOT_LOADED:
      return "not_loaded";
    case ModelLoadState.UNSUPPORTED:
      return "unsupported";
    default:
      return `unrecognised-${state}`;
  }
}

/** How a load state reads to an operator. */
function loadStateLabel(state: ModelLoadState): string {
  switch (state) {
    case ModelLoadState.LOADED:
      return "Resident";
    case ModelLoadState.NOT_LOADED:
      return "Not resident";
    case ModelLoadState.UNSUPPORTED:
      return "Residency n/a";
    default:
      return unrecognisedEnumText("residency", state);
  }
}

/**
 * Whether a model can hold a conversation: the daemon has to have said so, by labelling the model
 * `llm`. A positive label is required rather than "anything not labelled `embedding`", because the
 * daemon labels a model whose capabilities it could not determine `unknown` — deliberately refusing
 * to guess (PRD § Model catalog). Offering Chat on an `unknown` model would put that guess back,
 * and an OpenAI embedding model, which arrives labelled exactly that way, would get one.
 */
export function isChatCapable(model: ModelRow): boolean {
  return model.labels.includes("llm");
}

/** The `data-testid` stem of a model row. A model id may contain a colon (`qwen3:32b`). */
export function modelRowTestId(model: ModelRow): string {
  return `models-row-${model.daemonInstanceId}-${model.providerId}-${safeTestIdPart(model.modelId)}`;
}

const actionClassName =
  "rounded-md border border-input px-2 py-1 text-xs font-medium text-foreground hover:bg-accent";

export interface ModelsTableProps {
  models: ModelRow[];
  failures: DaemonFailure[];
  modelErrors: ReadonlyMap<string, string>;
  /**
   * Enumeration errors by {@link providerRowKey}. A provider that could not be enumerated has a
   * catalog nobody has been able to confirm since, so its rows are marked stale.
   */
  providerErrors: ReadonlyMap<string, string>;
  /** Why the table is empty, when it is — "still reading" is not the same claim as "no models". */
  status: RegistryReadStatus;
  onLoadModel: (model: ModelRow) => void;
  onUnloadModel: (model: ModelRow) => void;
  onOpenChat: (model: ModelRow) => void;
  onCreateAssistant: (model: ModelRow) => void;
}

export function ModelsTable({
  models,
  failures,
  modelErrors,
  providerErrors,
  status,
  onLoadModel,
  onUnloadModel,
  onOpenChat,
  onCreateAssistant,
}: ModelsTableProps) {
  return (
    <div className="rounded-md border border-border">
      <table className="w-full text-left text-sm text-foreground">
        <thead className="text-xs text-muted-foreground">
          <tr>
            <th className="px-3 py-2 font-medium">Model</th>
            <th className="px-3 py-2 font-medium">Daemon</th>
            <th className="px-3 py-2 font-medium">Provider</th>
            <th className="px-3 py-2 font-medium">Capabilities</th>
            <th className="px-3 py-2 font-medium">Residency</th>
            <th className="px-3 py-2 font-medium">Actions</th>
          </tr>
        </thead>
        <tbody>
          {failures.map((failure) => (
            <tr key={`failure-${failure.instanceId}`} className="border-t border-border">
              <td
                colSpan={6}
                data-testid={`models-daemon-error-${failure.instanceId}`}
                className="px-3 py-2 text-destructive"
              >
                {failure.instanceId}: {failure.error}
              </td>
            </tr>
          ))}
          {models.map((model) => (
            <ModelTableRow
              key={modelRowKey(model)}
              model={model}
              error={modelErrors.get(modelRowKey(model)) ?? ""}
              stale={providerErrors.has(providerRowKey(model))}
              onLoadModel={onLoadModel}
              onUnloadModel={onUnloadModel}
              onOpenChat={onOpenChat}
              onCreateAssistant={onCreateAssistant}
            />
          ))}
          {models.length === 0 && failures.length === 0 ? (
            <tr className="border-t border-border">
              <td
                colSpan={6}
                data-testid="models-table-empty"
                data-registry-status={status}
                className="px-3 py-2 text-muted-foreground"
              >
                {registryEmptyStateText(status, {
                  loading: "Reading the model catalog…",
                  ready: "No models",
                })}
              </td>
            </tr>
          ) : null}
        </tbody>
      </table>
    </div>
  );
}

function ModelTableRow({
  model,
  error,
  stale,
  onLoadModel,
  onUnloadModel,
  onOpenChat,
  onCreateAssistant,
}: {
  model: ModelRow;
  error: string;
  /** The owning provider's last enumeration failed, so this row is the last catalog that worked. */
  stale: boolean;
  onLoadModel: (model: ModelRow) => void;
  onUnloadModel: (model: ModelRow) => void;
  onOpenChat: (model: ModelRow) => void;
  onCreateAssistant: (model: ModelRow) => void;
}) {
  const testId = modelRowTestId(model);
  return (
    <tr data-testid={testId} data-stale={String(stale)} className="border-t border-border align-top">
      <td className="px-3 py-2">
        <div className="font-medium">{model.label}</div>
        <div className="text-xs text-muted-foreground">{model.modelId}</div>
        {stale ? (
          <div data-testid={`${testId}-stale`} className="text-xs text-destructive">
            Stale — last enumeration failed
          </div>
        ) : null}
      </td>
      <td data-testid={`${testId}-daemon`} className="px-3 py-2">
        {model.daemonInstanceId}
      </td>
      <td className="px-3 py-2">{model.providerId}</td>
      <td
        data-testid={`${testId}-labels`}
        data-model-labels={model.labels.join(",")}
        className="px-3 py-2 text-xs text-muted-foreground"
      >
        {model.labels.join(" · ")}
      </td>
      <td
        data-testid={`${testId}-load-state`}
        data-load-state={loadStateName(model.loadState)}
        className="px-3 py-2 text-xs"
      >
        {loadStateLabel(model.loadState)}
      </td>
      <td className="px-3 py-2">
        <div className="flex flex-wrap items-center gap-2">
          {model.loadState === ModelLoadState.NOT_LOADED ? (
            <button
              type="button"
              data-testid={`${testId}-load`}
              className={actionClassName}
              onClick={() => onLoadModel(model)}
            >
              Load
            </button>
          ) : null}
          {model.loadState === ModelLoadState.LOADED ? (
            <button
              type="button"
              data-testid={`${testId}-unload`}
              className={actionClassName}
              onClick={() => onUnloadModel(model)}
            >
              Unload
            </button>
          ) : null}
          {isChatCapable(model) ? (
            <button
              type="button"
              data-testid={`${testId}-chat`}
              className={actionClassName}
              onClick={() => onOpenChat(model)}
            >
              Chat
            </button>
          ) : null}
          <button
            type="button"
            data-testid={`${testId}-create-assistant`}
            className={actionClassName}
            onClick={() => onCreateAssistant(model)}
          >
            Create assistant
          </button>
        </div>
        {error ? (
          <div data-testid={`${testId}-error`} className="mt-1 text-xs text-destructive">
            {error}
          </div>
        ) : null}
      </td>
    </tr>
  );
}
