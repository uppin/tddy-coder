/**
 * Builders and an in-memory `ModelRegistryService` for the Models & Agents acceptance tests.
 *
 * Mirrors `connectionServiceBackend.ts`: a stateful in-memory fake rather than a wall of stubs, so
 * a load/unload actually changes what the next `ListModels` returns and a created assistant
 * actually shows up in `ListAssistants`.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md.
 */

import { create } from "@bufbuild/protobuf";
import { Code, ConnectError } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  ListProjectsResponseSchema,
  ProjectEntrySchema,
  type ProjectEntry,
} from "../../../src/gen/connection_pb";
import {
  ModelLoadState,
  ModelRegistryService,
  ProviderKind,
  type AssignableTool,
  type AssistantEntry,
  type ModelEntry,
  type ProviderEntry,
} from "../../../src/gen/models_pb";

/** The daemon a fixture belongs to unless a builder overrides it. */
export const FIXTURE_DAEMON = "workstation-1";

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/** A keyless local Ollama provider — the default any test starts from. */
export function anOllamaProvider(overrides: Partial<ProviderEntry> = {}): ProviderEntry {
  return {
    providerId: "prov-ollama",
    kind: ProviderKind.OLLAMA,
    label: "Local Ollama",
    baseUrl: "http://localhost:11434",
    hasCredential: false,
    daemonInstanceId: FIXTURE_DAEMON,
    enumerationError: "",
    ...overrides,
  } as ProviderEntry;
}

/** A cloud provider — models on it are never resident, so load/unload is unsupported. */
export function aCloudProvider(overrides: Partial<ProviderEntry> = {}): ProviderEntry {
  return {
    providerId: "prov-fireworks",
    kind: ProviderKind.FIREWORKS,
    label: "Fireworks",
    baseUrl: "https://api.fireworks.ai/inference",
    hasCredential: true,
    daemonInstanceId: FIXTURE_DAEMON,
    enumerationError: "",
    ...overrides,
  } as ProviderEntry;
}

/** A chat-capable local model that is not currently resident. */
export function anLlmModel(overrides: Partial<ModelEntry> = {}): ModelEntry {
  return {
    modelId: "qwen3:32b",
    providerId: "prov-ollama",
    label: "Qwen3 32B",
    labels: ["llm", "tools"],
    loadState: ModelLoadState.NOT_LOADED,
    daemonInstanceId: FIXTURE_DAEMON,
    sizeBytes: 20_000_000_000n,
    ...overrides,
  } as ModelEntry;
}

/** An embedding model — no chat surface applies to it. */
export function anEmbeddingModel(overrides: Partial<ModelEntry> = {}): ModelEntry {
  return {
    modelId: "nomic-embed-text",
    providerId: "prov-ollama",
    label: "Nomic Embed Text",
    labels: ["embedding"],
    loadState: ModelLoadState.NOT_LOADED,
    daemonInstanceId: FIXTURE_DAEMON,
    sizeBytes: 274_000_000n,
    ...overrides,
  } as ModelEntry;
}

/** A cloud model — residency does not apply. */
export function aCloudModel(overrides: Partial<ModelEntry> = {}): ModelEntry {
  return {
    modelId: "accounts/fireworks/models/kimi-k2",
    providerId: "prov-fireworks",
    label: "Kimi K2",
    labels: ["llm", "tools"],
    loadState: ModelLoadState.UNSUPPORTED,
    daemonInstanceId: FIXTURE_DAEMON,
    sizeBytes: 0n,
    ...overrides,
  } as ModelEntry;
}

export function anAssistant(overrides: Partial<AssistantEntry> = {}): AssistantEntry {
  return {
    assistantId: "asst-1",
    name: "repo-reader",
    label: "Repo Reader",
    providerId: "prov-ollama",
    modelId: "qwen3:32b",
    systemPrompt: "You read code and answer questions about it.",
    tools: ["Read", "Grep"],
    replaces: [],
    daemonInstanceId: FIXTURE_DAEMON,
    ...overrides,
  } as AssistantEntry;
}

/**
 * A project on the daemon that owns it — the workspace an assistant's tools may run in.
 *
 * `mainRepoPath` is the load-bearing field: the daemon resolves a chat's `cwd` against the very
 * same `projects.yaml` rows, so this is the one path a tool-bearing assistant's chat can name and
 * have accepted.
 */
export function aProject(overrides: Partial<Omit<ProjectEntry, "$typeName">> = {}): ProjectEntry {
  return create(ProjectEntrySchema, {
    projectId: "proj-1",
    name: "tddy-coder",
    gitUrl: "https://github.com/test/tddy-coder.git",
    mainRepoPath: "/home/dev/Code/tddy-coder",
    daemonInstanceId: FIXTURE_DAEMON,
    ...overrides,
  });
}

/** What a daemon answers `ConnectionService.ListProjects` with. */
export function listedProjects(projects: ProjectEntry[]) {
  return create(ListProjectsResponseSchema, { projects });
}

/** The exec catalog as the daemon advertises it — the web renders no tool list of its own. */
export const EXEC_CATALOG: AssignableTool[] = (
  [
    ["Read", false],
    ["Write", true],
    ["StrReplace", true],
    ["Delete", true],
    ["Grep", false],
    ["Glob", false],
    ["Shell", true],
    ["Await", false],
    ["ReadLints", false],
    ["SemanticSearch", false],
  ] as Array<[string, boolean]>
).map(([name, isMutating]) => ({ name, description: `${name} tool`, isMutating }) as AssignableTool);

// ---------------------------------------------------------------------------
// Backend
// ---------------------------------------------------------------------------

export interface ModelRegistryFixture {
  providers?: ProviderEntry[];
  models?: ModelEntry[];
  assistants?: AssistantEntry[];
  /** Tool catalog the daemon advertises; defaults to the full exec catalog. */
  assignableTools?: AssignableTool[];
}

/**
 * A stateful in-memory `ModelRegistryService`. Load/unload mutate the seeded model's state so the
 * re-rendered row reflects the daemon's answer; a load/unload on a model whose provider has no
 * notion of residency is rejected with `FAILED_PRECONDITION`, matching the proto contract.
 */
export function aModelRegistryBackend(fixture: ModelRegistryFixture = {}): InMemoryRpcBackend {
  const providers = [...(fixture.providers ?? [anOllamaProvider()])];
  const models = [...(fixture.models ?? [])];
  const assistants = [...(fixture.assistants ?? [])];
  const assignableTools = fixture.assignableTools ?? EXEC_CATALOG;

  const findModel = (providerId: string, modelId: string) => {
    const model = models.find((m) => m.providerId === providerId && m.modelId === modelId);
    if (!model) {
      throw new ConnectError(`no such model: ${providerId}/${modelId}`, Code.NotFound);
    }
    return model;
  };

  /**
   * The index of the row `id` names, or `NOT_FOUND` — never `-1` handed to `splice`, which deletes
   * the *last* row and would leave a spec asserting against a corrupted registry.
   */
  const indexOf = <T,>(rows: T[], matches: (row: T) => boolean, describe: string) => {
    const index = rows.findIndex(matches);
    if (index < 0) {
      throw new ConnectError(describe, Code.NotFound);
    }
    return index;
  };

  const setLoadState = (providerId: string, modelId: string, state: ModelLoadState) => {
    const model = findModel(providerId, modelId);
    if (model.loadState === ModelLoadState.UNSUPPORTED) {
      throw new ConnectError(
        "residency is not supported for this provider kind",
        Code.FailedPrecondition,
      );
    }
    model.loadState = state;
    return { model };
  };

  return anInMemoryRpcBackend()
    .onUnary(ModelRegistryService.method.listProviders, () => ({ providers }))
    .onUnary(ModelRegistryService.method.createProvider, (req) => {
      const provider = anOllamaProvider({
        providerId: `prov-${providers.length + 1}`,
        kind: req.kind,
        label: req.label,
        baseUrl: req.baseUrl,
        hasCredential: req.apiKey.length > 0,
      });
      providers.push(provider);
      return { provider };
    })
    .onUnary(ModelRegistryService.method.deleteProvider, (req) => {
      const index = indexOf(
        providers,
        (p) => p.providerId === req.providerId,
        `no such provider: ${req.providerId}`,
      );
      // A provider goes with its cached models, as `DeleteProvider` documents — a fake that kept
      // them would let a screen pass that leaves rows behind pointing at a provider that is gone.
      providers.splice(index, 1);
      for (let i = models.length - 1; i >= 0; i -= 1) {
        if (models[i].providerId === req.providerId) models.splice(i, 1);
      }
      return {};
    })
    .onUnary(ModelRegistryService.method.listModels, () => ({ models }))
    .onUnary(ModelRegistryService.method.refreshProviderModels, (req) => ({
      models: models.filter((m) => m.providerId === req.providerId),
    }))
    .onUnary(ModelRegistryService.method.loadModel, (req) =>
      setLoadState(req.providerId, req.modelId, ModelLoadState.LOADED),
    )
    .onUnary(ModelRegistryService.method.unloadModel, (req) =>
      setLoadState(req.providerId, req.modelId, ModelLoadState.NOT_LOADED),
    )
    .onUnary(ModelRegistryService.method.listAssistants, () => ({ assistants }))
    .onUnary(ModelRegistryService.method.createAssistant, (req) => {
      const assistant = anAssistant({
        assistantId: `asst-${assistants.length + 1}`,
        name: req.name,
        label: req.label,
        providerId: req.providerId,
        modelId: req.modelId,
        systemPrompt: req.systemPrompt,
        tools: req.tools,
        replaces: req.replaces,
      });
      assistants.push(assistant);
      return { assistant };
    })
    .onUnary(ModelRegistryService.method.updateAssistant, (req) => {
      const assistant =
        assistants[
          indexOf(
            assistants,
            (a) => a.assistantId === req.assistantId,
            `no such assistant: ${req.assistantId}`,
          )
        ];
      assistant.label = req.label;
      assistant.systemPrompt = req.systemPrompt;
      assistant.tools = req.tools;
      assistant.replaces = req.replaces;
      return { assistant };
    })
    .onUnary(ModelRegistryService.method.deleteAssistant, (req) => {
      const index = indexOf(
        assistants,
        (a) => a.assistantId === req.assistantId,
        `no such assistant: ${req.assistantId}`,
      );
      assistants.splice(index, 1);
      return {};
    })
    .onUnary(ModelRegistryService.method.listAssignableTools, () => ({ tools: assignableTools }));
}
