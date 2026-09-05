/**
 * Cross-daemon fan-out for `models.ModelRegistryService`.
 *
 * Every RPC in that service answers only for the daemon serving it — the daemon never forwards to
 * its peers (see `models.proto`) — so the fleet-wide registry is assembled here: one client per
 * common-room daemon (`daemon-{instanceId}` over the shared common-room connection, the same
 * addressing `useDaemonClientFor` performs), each daemon read independently, and the answers merged
 * by `utils/mergeRegistryEntries`.
 *
 * Two properties this hook exists to guarantee:
 *   • **a per-row action reaches the row's owning daemon**, not the selected one — the client is
 *     chosen by the row's `daemonInstanceId`;
 *   • **one unreachable daemon costs one error row**, not the whole page — each daemon's read is
 *     isolated, and its failure becomes a `DaemonFailure` rather than an empty registry.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md § AC2, AC5, AC12.
 */

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import { Code, ConnectError, type Client } from "@connectrpc/connect";
import {
  ModelRegistryService,
  type AssignableTool,
  type AssistantEntry,
  type ModelEntry,
  type ProviderEntry,
  type ProviderKind,
} from "../../gen/models_pb";
import { useAuthContext } from "../../hooks/authProvider";
import { useHostConnector } from "../../rpc/connections/registry";
import { useSelectedDaemon } from "../../rpc/selectedDaemon";
// The same wording every fleet-wide read reports an unreachable host with.
import { noConnectionTo } from "../../rpc/useHostFanOut";
import {
  assistantRowKey,
  describeReadFailures,
  mergeRegistryEntries,
  modelRowKey,
  owningDaemonOf,
  providerRowKey,
  registryReadStatus,
  type AssistantRow,
  type DaemonFailure,
  type DaemonRegistrySnapshot,
  type ModelRow,
  type ProviderRow,
  type RegistryListFailure,
  type RegistryReadStatus,
} from "../../utils/mergeRegistryEntries";

type RegistryClient = Client<typeof ModelRegistryService>;

/** One daemon's registry as this hook holds it, plus the tool catalog that daemon advertises. */
interface DaemonState extends DaemonRegistrySnapshot {
  readonly assignableTools: readonly AssignableTool[];
  /** Non-empty when `ListAssignableTools` failed — the difference between "no tools" and "unknown". */
  readonly toolsError: string;
}

/**
 * A daemon's exec catalog as the screen knows it. An empty list is a claim ("this daemon assigns no
 * tools"), and a failed read must not be able to make that claim — an assistant created from it
 * would be persisted toolless — so the three cases are distinct rather than all being `[]`.
 */
export type ToolCatalog =
  | { readonly status: "loading" }
  | { readonly status: "unavailable"; readonly error: string }
  | { readonly status: "ready"; readonly tools: readonly AssignableTool[] };

/** The catalog of a daemon that has not answered yet — one value, so identity is stable. */
const TOOLS_LOADING: ToolCatalog = { status: "loading" };

/** What a screen needs to render and act on the fleet's model registry. */
export interface ModelRegistryFanOut {
  readonly providers: ProviderRow[];
  readonly models: ModelRow[];
  readonly assistants: AssistantRow[];
  readonly failures: DaemonFailure[];
  /** How far the fleet-wide read has got — an empty catalog alone cannot say why it is empty. */
  readonly status: RegistryReadStatus;
  /** The exec catalog the given daemon advertises — the web holds no tool list of its own. */
  readonly toolsFor: (daemonInstanceId: string) => ToolCatalog;
  /**
   * Enumeration errors keyed by {@link providerRowKey}: the daemon's own, plus any failed refresh
   * from this screen. These mark the provider's models stale, so nothing but a failure to enumerate
   * belongs here.
   */
  readonly providerErrors: ReadonlyMap<string, string>;
  /**
   * Errors from a write against a provider (deletion), keyed by {@link providerRowKey}. Held apart
   * from {@link providerErrors} because a refused delete says nothing about the catalog's freshness.
   */
  readonly providerActionErrors: ReadonlyMap<string, string>;
  /** Errors from a write against an assistant, keyed by {@link assistantRowKey}. */
  readonly assistantErrors: ReadonlyMap<string, string>;
  /** Errors from a per-model action, keyed by {@link modelRowKey}. */
  readonly modelErrors: ReadonlyMap<string, string>;
  readonly loadModel: (model: ModelRow) => Promise<boolean>;
  readonly unloadModel: (model: ModelRow) => Promise<boolean>;
  readonly refreshProvider: (provider: ProviderRow) => Promise<void>;
  /** Remove a provider from the daemon that owns it, with its cached models. */
  readonly deleteProvider: (provider: ProviderRow) => Promise<void>;
  /** Resolves to the error to show, or `""` when the provider was created. */
  readonly createProvider: (input: {
    daemonInstanceId: string;
    kind: ProviderKind;
    label: string;
    baseUrl: string;
    apiKey: string;
  }) => Promise<string>;
  /** Resolves to the error to show, or `""` when the assistant was created. */
  readonly createAssistant: (input: {
    daemonInstanceId: string;
    name: string;
    label: string;
    providerId: string;
    modelId: string;
    systemPrompt: string;
    tools: string[];
    replaces: string[];
  }) => Promise<string>;
  /**
   * Resolves to the error to show, or `""` when the assistant was updated. Both tool sets are sent
   * whole, so an operator who gives up a takeover is obeyed rather than left with the stored one.
   */
  readonly updateAssistant: (input: {
    assistant: AssistantRow;
    label: string;
    systemPrompt: string;
    tools: string[];
    replaces: string[];
  }) => Promise<string>;
  /** Remove an assistant from the daemon that owns it. */
  readonly deleteAssistant: (assistant: AssistantRow) => Promise<void>;
}

/**
 * The message to show for a failed RPC: the daemon's own words, never a swallowed error.
 *
 * A registry write is served only by the daemon that owns the row, so a refusal to write is the one
 * failure an operator can act on by going to the right host — it is named rather than left to read
 * like any other error.
 */
export function errorTextOf(err: unknown): string {
  const failure = ConnectError.from(err);
  const message = failure.rawMessage || String(err);
  return failure.code === Code.PermissionDenied ? `Permission denied — ${message}` : message;
}

function providerRowOf(entry: ProviderEntry, sourceInstanceId: string): ProviderRow {
  return {
    daemonInstanceId: owningDaemonOf(entry.daemonInstanceId, sourceInstanceId),
    providerId: entry.providerId,
    kind: entry.kind,
    label: entry.label,
    baseUrl: entry.baseUrl,
    hasCredential: entry.hasCredential,
    enumerationError: entry.enumerationError,
  };
}

function modelRowOf(entry: ModelEntry, sourceInstanceId: string): ModelRow {
  return {
    daemonInstanceId: owningDaemonOf(entry.daemonInstanceId, sourceInstanceId),
    providerId: entry.providerId,
    modelId: entry.modelId,
    label: entry.label,
    labels: entry.labels,
    loadState: entry.loadState,
    sizeBytes: entry.sizeBytes,
  };
}

function assistantRowOf(entry: AssistantEntry, sourceInstanceId: string): AssistantRow {
  return {
    daemonInstanceId: owningDaemonOf(entry.daemonInstanceId, sourceInstanceId),
    assistantId: entry.assistantId,
    name: entry.name,
    label: entry.label,
    providerId: entry.providerId,
    modelId: entry.modelId,
    systemPrompt: entry.systemPrompt,
    tools: entry.tools,
    replaces: entry.replaces,
  };
}

/**
 * Read — and act on — the model registry of every daemon in the common room.
 *
 * Returns `null`-safe results throughout: until a wire that can reach a daemon is registered there
 * is no client for it, so every read and every action reports that instead of being issued against a
 * connection that does not exist (`useDaemonClientFor`'s contract) — and instead of leaving the
 * screen with an empty catalog that reads as "this fleet has no models".
 */
export function useModelRegistryFanOut(): ModelRegistryFanOut {
  const { sessionToken } = useAuthContext();
  const { daemons, roomStatus } = useSelectedDaemon();
  const connectHost = useHostConnector();
  const token = sessionToken ?? "";

  const [statesByDaemon, setStatesByDaemon] = useState<ReadonlyMap<string, DaemonState>>(new Map());
  const [refreshErrors, setRefreshErrors] = useState<ReadonlyMap<string, string>>(new Map());
  const [providerActionErrors, setProviderActionErrors] = useState<ReadonlyMap<string, string>>(
    new Map(),
  );
  const [assistantErrors, setAssistantErrors] = useState<ReadonlyMap<string, string>>(new Map());
  const [modelErrors, setModelErrors] = useState<ReadonlyMap<string, string>>(new Map());

  // Resolve each daemon through the connection registry rather than through `useDaemonClientFor`,
  // because the daemon list is dynamic — one hook per daemon would change the hook count between
  // renders. A host no registered wire can reach has no client, which is what every read below
  // reports as `noConnectionTo`.
  const clientFor = useCallback(
    (instanceId: string): RegistryClient | null =>
      connectHost(instanceId)?.clientFor(ModelRegistryService) ?? null,
    [connectHost],
  );

  /**
   * Whether this hook is still mounted. A read that lands after the screen is gone has nobody to
   * render it, and writing it would be a state update against a dead component.
   */
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const readDaemon = useCallback(
    async (instanceId: string, signal?: AbortSignal): Promise<void> => {
      const put = (state: DaemonState) => {
        if (!mounted.current || signal?.aborted) return;
        setStatesByDaemon((current) => new Map(current).set(instanceId, state));
        // A provider that has just enumerated cleanly is current again, so the failure a previous
        // refresh recorded against it is spent. Left behind, it would mark every one of that
        // provider's models "Stale — last enumeration failed" for as long as the screen is open,
        // however many times the daemon has since answered.
        const enumerating = state.providers
          .filter((p) => p.enumerationError === "")
          .map(providerRowKey);
        if (enumerating.length === 0) return;
        setRefreshErrors((current) => {
          if (!enumerating.some((key) => current.has(key))) return current;
          const next = new Map(current);
          for (const key of enumerating) next.delete(key);
          return next;
        });
      };
      const client = clientFor(instanceId);
      if (!client) {
        put({
          instanceId,
          providers: [],
          models: [],
          assistants: [],
          assignableTools: [],
          toolsError: noConnectionTo(instanceId),
          error: noConnectionTo(instanceId),
        });
        return;
      }
      // Four independent lists, settled independently: one that fails costs that list and says so,
      // while the others still render. `Promise.all` would blank a daemon's providers, models *and*
      // assistants because one of the four was rejected.
      const [providers, models, assistants, tools] = await Promise.allSettled([
        client.listProviders({ sessionToken: token }, { signal }),
        client.listModels({ sessionToken: token }, { signal }),
        client.listAssistants({ sessionToken: token }, { signal }),
        client.listAssignableTools({ sessionToken: token }, { signal }),
      ]);
      const failures: RegistryListFailure[] = [];
      const rowsOf = <T, R>(
        result: PromiseSettledResult<T>,
        list: string,
        select: (value: T) => R[],
      ): R[] => {
        if (result.status === "fulfilled") return select(result.value);
        failures.push({ list, message: errorTextOf(result.reason) });
        return [];
      };
      const providerRows = rowsOf(providers, "providers", (r) =>
        r.providers.map((p) => providerRowOf(p, instanceId)),
      );
      const modelRows = rowsOf(models, "models", (r) =>
        r.models.map((m) => modelRowOf(m, instanceId)),
      );
      const assistantRows = rowsOf(assistants, "assistants", (r) =>
        r.assistants.map((a) => assistantRowOf(a, instanceId)),
      );
      // The tool catalog is the one list whose own failure has to be kept: a caller that cannot see
      // it would read the empty list as "this daemon assigns no tools" and let an assistant be
      // created toolless from a read that never arrived.
      const toolsError = tools.status === "rejected" ? errorTextOf(tools.reason) : "";
      const toolRows = rowsOf(tools, "tools", (r) => [...r.tools]);
      put({
        instanceId,
        providers: providerRows,
        models: modelRows,
        assistants: assistantRows,
        assignableTools: toolRows,
        toolsError,
        error: describeReadFailures(failures),
      });
    },
    [clientFor, token],
  );

  // The daemon list is rebuilt on every common-room participant event, so its array identity changes
  // far more often than its contents. Depending on the ids themselves keeps the fan-out to one read
  // per daemon per actual change, instead of four RPCs per daemon per re-render. `\n` cannot occur in
  // a daemon instance id (they are LiveKit participant identity segments).
  const daemonIdsKey = daemons.map((d) => d.instanceId).join("\n");
  const daemonIds = useMemo(
    () => daemonIdsKey.split("\n").filter((id) => id !== ""),
    [daemonIdsKey],
  );

  useEffect(() => {
    const reads = new AbortController();
    for (const instanceId of daemonIds) {
      void readDaemon(instanceId, reads.signal);
    }
    // Unmounting — or moving on to a different daemon list — cancels the reads in flight, so no
    // answer to a question nobody is asking any more is waited for or written.
    return () => reads.abort();
  }, [daemonIds, readDaemon]);

  const merged = useMemo(() => {
    const snapshots = daemonIds
      .map((instanceId) => statesByDaemon.get(instanceId))
      .filter((s): s is DaemonState => s !== undefined);
    return mergeRegistryEntries(snapshots);
  }, [daemonIds, statesByDaemon]);

  // The common room is this fleet's host directory: joined or joining, it can name hosts; idle or
  // failed, it names none and the screen has nothing to be empty *of*.
  const hasDirectory = roomStatus === "connecting" || roomStatus === "connected";
  const status = useMemo(
    () =>
      registryReadStatus({
        // "Connected" is the claim that this fleet can be addressed at all, and two different
        // things have to hold for it: the directory that names the hosts is up, and the hosts it
        // names are reachable. An empty fleet is only "no daemons" when the directory is up —
        // otherwise there are no daemons *known*, which is a disconnection and says so.
        connected: hasDirectory && daemonIds.every((instanceId) => connectHost(instanceId) !== null),
        daemonCount: daemonIds.length,
        answeredCount: daemonIds.filter((instanceId) => statesByDaemon.has(instanceId)).length,
      }),
    [hasDirectory, connectHost, daemonIds, statesByDaemon],
  );

  const toolsFor = useCallback(
    (daemonInstanceId: string): ToolCatalog => {
      const state = statesByDaemon.get(daemonInstanceId);
      if (!state) return TOOLS_LOADING;
      if (state.toolsError) return { status: "unavailable", error: state.toolsError };
      return { status: "ready", tools: state.assignableTools };
    },
    [statesByDaemon],
  );

  const providerErrors = useMemo(() => {
    const errors = new Map<string, string>();
    for (const provider of merged.providers) {
      if (provider.enumerationError) {
        errors.set(providerRowKey(provider), provider.enumerationError);
      }
    }
    for (const [key, error] of refreshErrors) errors.set(key, error);
    return errors;
  }, [merged.providers, refreshErrors]);

  /** Record — or, given `""`, clear — the error shown against one row of an error map. */
  const recordRowError = useCallback(
    (
      setErrors: Dispatch<SetStateAction<ReadonlyMap<string, string>>>,
      key: string,
      error: string,
    ) =>
      setErrors((current) => {
        const next = new Map(current);
        if (error) next.set(key, error);
        else next.delete(key);
        return next;
      }),
    [],
  );

  const recordModelError = useCallback(
    (model: ModelRow, error: string) => recordRowError(setModelErrors, modelRowKey(model), error),
    [recordRowError],
  );

  const setResidency = useCallback(
    async (model: ModelRow, resident: boolean): Promise<boolean> => {
      const client = clientFor(model.daemonInstanceId);
      if (!client) {
        recordModelError(model, noConnectionTo(model.daemonInstanceId));
        return false;
      }
      const request = {
        sessionToken: token,
        providerId: model.providerId,
        modelId: model.modelId,
      };
      try {
        await (resident ? client.loadModel(request) : client.unloadModel(request));
        recordModelError(model, "");
      } catch (err) {
        recordModelError(model, errorTextOf(err));
        return false;
      }
      await readDaemon(model.daemonInstanceId);
      return true;
    },
    [clientFor, token, recordModelError, readDaemon],
  );

  const loadModel = useCallback((model: ModelRow) => setResidency(model, true), [setResidency]);
  const unloadModel = useCallback((model: ModelRow) => setResidency(model, false), [setResidency]);

  const refreshProvider = useCallback(
    async (provider: ProviderRow): Promise<void> => {
      const recordRefreshError = (error: string) =>
        recordRowError(setRefreshErrors, providerRowKey(provider), error);
      const client = clientFor(provider.daemonInstanceId);
      if (!client) {
        // The operator asked for a refresh and is owed an answer either way; silently doing
        // nothing would read as "refreshed, still the same".
        recordRefreshError(noConnectionTo(provider.daemonInstanceId));
        return;
      }
      try {
        await client.refreshProviderModels({
          sessionToken: token,
          providerId: provider.providerId,
        });
        recordRefreshError("");
      } catch (err) {
        recordRefreshError(errorTextOf(err));
        return;
      }
      await readDaemon(provider.daemonInstanceId);
    },
    [clientFor, token, readDaemon, recordRowError],
  );

  const deleteProvider = useCallback(
    async (provider: ProviderRow): Promise<void> => {
      const key = providerRowKey(provider);
      const client = clientFor(provider.daemonInstanceId);
      if (!client) {
        recordRowError(setProviderActionErrors, key, noConnectionTo(provider.daemonInstanceId));
        return;
      }
      try {
        await client.deleteProvider({ sessionToken: token, providerId: provider.providerId });
        recordRowError(setProviderActionErrors, key, "");
      } catch (err) {
        // The daemon refuses a delete that would orphan an assistant, and refuses a write it does
        // not own — either way the provider is still there, so the row has to say why.
        recordRowError(setProviderActionErrors, key, errorTextOf(err));
        return;
      }
      await readDaemon(provider.daemonInstanceId);
    },
    [clientFor, token, readDaemon, recordRowError],
  );

  const createProvider = useCallback<ModelRegistryFanOut["createProvider"]>(
    async ({ daemonInstanceId, kind, label, baseUrl, apiKey }) => {
      const client = clientFor(daemonInstanceId);
      if (!client) return noConnectionTo(daemonInstanceId);
      try {
        await client.createProvider({ sessionToken: token, kind, label, baseUrl, apiKey });
      } catch (err) {
        return errorTextOf(err);
      }
      await readDaemon(daemonInstanceId);
      return "";
    },
    [clientFor, token, readDaemon],
  );

  const createAssistant = useCallback<ModelRegistryFanOut["createAssistant"]>(
    async ({ daemonInstanceId, name, label, providerId, modelId, systemPrompt, tools, replaces }) => {
      const client = clientFor(daemonInstanceId);
      if (!client) return noConnectionTo(daemonInstanceId);
      try {
        await client.createAssistant({
          sessionToken: token,
          name,
          label,
          providerId,
          modelId,
          systemPrompt,
          tools,
          replaces,
        });
      } catch (err) {
        return errorTextOf(err);
      }
      await readDaemon(daemonInstanceId);
      return "";
    },
    [clientFor, token, readDaemon],
  );

  const updateAssistant = useCallback<ModelRegistryFanOut["updateAssistant"]>(
    async ({ assistant, label, systemPrompt, tools, replaces }) => {
      const client = clientFor(assistant.daemonInstanceId);
      if (!client) return noConnectionTo(assistant.daemonInstanceId);
      try {
        await client.updateAssistant({
          sessionToken: token,
          assistantId: assistant.assistantId,
          label,
          systemPrompt,
          tools,
          replaces,
        });
      } catch (err) {
        return errorTextOf(err);
      }
      await readDaemon(assistant.daemonInstanceId);
      return "";
    },
    [clientFor, token, readDaemon],
  );

  const deleteAssistant = useCallback(
    async (assistant: AssistantRow): Promise<void> => {
      const key = assistantRowKey(assistant);
      const client = clientFor(assistant.daemonInstanceId);
      if (!client) {
        recordRowError(setAssistantErrors, key, noConnectionTo(assistant.daemonInstanceId));
        return;
      }
      try {
        await client.deleteAssistant({
          sessionToken: token,
          assistantId: assistant.assistantId,
        });
        recordRowError(setAssistantErrors, key, "");
      } catch (err) {
        recordRowError(setAssistantErrors, key, errorTextOf(err));
        return;
      }
      await readDaemon(assistant.daemonInstanceId);
    },
    [clientFor, token, readDaemon, recordRowError],
  );

  return {
    providers: merged.providers,
    models: merged.models,
    assistants: merged.assistants,
    failures: merged.failures,
    status,
    toolsFor,
    providerErrors,
    providerActionErrors,
    assistantErrors,
    modelErrors,
    loadModel,
    unloadModel,
    refreshProvider,
    deleteProvider,
    createProvider,
    createAssistant,
    updateAssistant,
    deleteAssistant,
  };
}
