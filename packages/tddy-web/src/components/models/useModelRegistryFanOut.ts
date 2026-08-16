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

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ConnectError, createClient, type Client } from "@connectrpc/connect";
import {
  ModelRegistryService,
  type AssignableTool,
  type AssistantEntry,
  type ModelEntry,
  type ProviderEntry,
  type ProviderKind,
} from "../../gen/models_pb";
import { useAuthContext } from "../../hooks/authProvider";
import { daemonRpcIdentity } from "../../lib/participantRole";
import { useSelectedDaemon } from "../../rpc/selectedDaemon";
import { useLiveKitTransportFactory } from "../../rpc/transportProvider";
import {
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
}

/** What a screen needs to render and act on the fleet's model registry. */
export interface ModelRegistryFanOut {
  readonly providers: ProviderRow[];
  readonly models: ModelRow[];
  readonly assistants: AssistantRow[];
  readonly failures: DaemonFailure[];
  /** How far the fleet-wide read has got — an empty catalog alone cannot say why it is empty. */
  readonly status: RegistryReadStatus;
  /** The exec catalog the given daemon advertises — the web holds no tool list of its own. */
  readonly toolsFor: (daemonInstanceId: string) => readonly AssignableTool[];
  /**
   * Enumeration errors keyed by {@link providerRowKey}: the daemon's own, plus any failed refresh
   * from this screen.
   */
  readonly providerErrors: ReadonlyMap<string, string>;
  /** Errors from a per-model action, keyed by {@link modelRowKey}. */
  readonly modelErrors: ReadonlyMap<string, string>;
  readonly loadModel: (model: ModelRow) => Promise<boolean>;
  readonly unloadModel: (model: ModelRow) => Promise<boolean>;
  readonly refreshProvider: (provider: ProviderRow) => Promise<void>;
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
  }) => Promise<string>;
}

/** The message to show for a failed RPC: the daemon's own words, never a swallowed error. */
function errorTextOf(err: unknown): string {
  const message = ConnectError.from(err).rawMessage;
  return message || String(err);
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
  };
}

const EMPTY_TOOLS: readonly AssignableTool[] = [];

/** What a caller is told when the common room holds no connection to the daemon it addressed. */
function noConnectionTo(daemonInstanceId: string): string {
  return `no connection to daemon ${daemonInstanceId}`;
}

/**
 * Read — and act on — the model registry of every daemon in the common room.
 *
 * Returns `null`-safe results throughout: until the common room is connected there is no client for
 * any daemon, so every read and every action reports that instead of being issued against a
 * connection that does not exist (`useDaemonClientFor`'s contract) — and instead of leaving the
 * screen with an empty catalog that reads as "this fleet has no models".
 */
export function useModelRegistryFanOut(): ModelRegistryFanOut {
  const { sessionToken } = useAuthContext();
  const { room, daemons } = useSelectedDaemon();
  const liveKitFactory = useLiveKitTransportFactory();
  const token = sessionToken ?? "";

  const [statesByDaemon, setStatesByDaemon] = useState<ReadonlyMap<string, DaemonState>>(new Map());
  const [refreshErrors, setRefreshErrors] = useState<ReadonlyMap<string, string>>(new Map());
  const [modelErrors, setModelErrors] = useState<ReadonlyMap<string, string>>(new Map());

  // Address each daemon's own RPC server over the shared common-room connection. Built per call
  // rather than through `useDaemonClientFor` because the daemon list is dynamic — one hook per
  // daemon would change the hook count between renders.
  const clientFor = useCallback(
    (instanceId: string): RegistryClient | null =>
      room && instanceId
        ? createClient(ModelRegistryService, liveKitFactory(room, daemonRpcIdentity(instanceId)))
        : null,
    [room, liveKitFactory],
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
      };
      const client = clientFor(instanceId);
      if (!client) {
        put({
          instanceId,
          providers: [],
          models: [],
          assistants: [],
          assignableTools: [],
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
      put({
        instanceId,
        providers: rowsOf(providers, "providers", (r) =>
          r.providers.map((p) => providerRowOf(p, instanceId)),
        ),
        models: rowsOf(models, "models", (r) => r.models.map((m) => modelRowOf(m, instanceId))),
        assistants: rowsOf(assistants, "assistants", (r) =>
          r.assistants.map((a) => assistantRowOf(a, instanceId)),
        ),
        assignableTools: rowsOf(tools, "tools", (r) => [...r.tools]),
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

  const status = useMemo(
    () =>
      registryReadStatus({
        connected: room !== null,
        daemonCount: daemonIds.length,
        answeredCount: daemonIds.filter((instanceId) => statesByDaemon.has(instanceId)).length,
      }),
    [room, daemonIds, statesByDaemon],
  );

  const toolsFor = useCallback(
    (daemonInstanceId: string) =>
      statesByDaemon.get(daemonInstanceId)?.assignableTools ?? EMPTY_TOOLS,
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

  const recordModelError = useCallback((model: ModelRow, error: string) => {
    setModelErrors((current) => {
      const next = new Map(current);
      if (error) next.set(modelRowKey(model), error);
      else next.delete(modelRowKey(model));
      return next;
    });
  }, []);

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
        setRefreshErrors((current) => {
          const next = new Map(current);
          if (error) next.set(providerRowKey(provider), error);
          else next.delete(providerRowKey(provider));
          return next;
        });
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
    [clientFor, token, readDaemon],
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
    async ({ daemonInstanceId, name, label, providerId, modelId, systemPrompt, tools }) => {
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
        });
      } catch (err) {
        return errorTextOf(err);
      }
      await readDaemon(daemonInstanceId);
      return "";
    },
    [clientFor, token, readDaemon],
  );

  return {
    providers: merged.providers,
    models: merged.models,
    assistants: merged.assistants,
    failures: merged.failures,
    status,
    toolsFor,
    providerErrors,
    modelErrors,
    loadModel,
    unloadModel,
    refreshProvider,
    createProvider,
    createAssistant,
  };
}
