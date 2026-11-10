/**
 * The clients port: how a component package reaches the daemon, a host, or plain HTTP — without
 * knowing which wire (HTTP, Tauri IPC, LiveKit) the app chose. The app supplies a {@link ClientsPort}
 * through {@link ClientsProvider}; the hooks below read it. The hook names are the ones `tddy-web`
 * used before the split, so a moved component changes only its import path.
 */
import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useSyncExternalStore,
  type ReactElement,
  type ReactNode,
} from "react";
import { createClient, type Client, type Transport } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import type { ConnectionProviderRegistry } from "../connections/registry";
import type { ConnectionStatus, HostConnection } from "../connections/types";
import { hostLabelOf } from "../hostLabel";

/** A daemon a client can be built for, as the host directory lists it. */
export interface DaemonHost {
  instanceId: string;
  label: string;
  /** The host's base clone location, relative to each OS user's home. Older daemons omit it. */
  reposBasePath?: string;
  /** The largest single session attachment this host will serve. Older daemons omit it. */
  maxAttachmentBytes?: number;
  /** What this host's workspace jail confines. Absent = the sandboxed-codebase placement is not served. */
  sandboxedCodebase?: { confinesFilesystem: boolean };
}

/** The daemon selection every daemon-scoped hook resolves against. */
export interface SelectedDaemonState {
  readonly daemons: DaemonHost[];
  readonly selectedInstanceId: string | null;
  readonly selectedLabel: string | null;
  readonly servingInstanceId?: string;
  readonly selectDaemon: (instanceId: string) => void;
  readonly directoryStatus: ConnectionStatus;
}

/** Everything the app supplies so component packages can build clients. */
export interface ClientsPort {
  /** The transport for services the serving daemon answers over plain HTTP (auth, config). */
  readonly httpTransport: Transport;
  /** The provider registry hosts are reached through. */
  readonly connections: ConnectionProviderRegistry;
  /** Whether `hostId` may be dialled yet (e.g. its room has joined). Every host is ready when absent. */
  readonly isHostReady?: (hostId: string) => boolean;
  /** The current daemon selection. */
  readonly selectedDaemon: SelectedDaemonState;
}

const ClientsContext = createContext<ClientsPort | null>(null);

/** Mounts `port` for the subtree. The hooks below throw, naming this component, when it is absent. */
export function ClientsProvider({ port, children }: { port: ClientsPort; children?: ReactNode }): ReactElement {
  return <ClientsContext.Provider value={port}>{children}</ClientsContext.Provider>;
}

/** The port in scope; throws naming the port when no {@link ClientsProvider} wraps the caller. */
export function useClientsPort(): ClientsPort {
  const port = useContext(ClientsContext);
  if (!port) {
    throw new Error("the clients port is not provided: wrap this component in a ClientsProvider");
  }
  return port;
}

/** The transport for services the serving daemon answers over plain HTTP (auth, config). */
export function useHttpTransport(): Transport {
  return useClientsPort().httpTransport;
}

export function useHttpClient<S extends DescService>(service: S): Client<S> {
  const transport = useHttpTransport();
  return useMemo(() => createClient(service, transport), [service, transport]);
}

/** The registry hosts are resolved through. */
export function useConnectionRegistry(): ConnectionProviderRegistry {
  return useClientsPort().connections;
}

/**
 * Resolve hosts named at call time, for a caller that cannot name them at render time.
 *
 * A fan-out reads a host list that changes, and a form addresses the host the operator just picked;
 * neither can spend a hook per host. They take this resolver instead and call it inside their
 * effect or callback. Its identity changes exactly when the routing does — a provider registering,
 * or re-registering with a new wire — so it is the dependency that replaces `[room, liveKitFactory]`
 * in those callers' dependency arrays, and it re-runs their reads for the same reasons those did.
 */
export function useHostConnector(): (hostId: string | null) => HostConnection | null {
  // Not compiled: the memo below is keyed on `revision`, which the resolver never reads. The React
  // Compiler memoises on what a body reads, so it would drop `revision`, keep the resolver from before
  // the common room joined, and leave every `useHostConnection` returning `null` for the page's life.
  "use no memo";
  const { connections: registry, isHostReady: isReady } = useClientsPort();
  const subscribe = useCallback((listener: () => void) => registry.subscribe(listener), [registry]);
  const revision = useSyncExternalStore(subscribe, () => registry.revision());
  // `revision` is a dependency, not an input — the resolver reads the registry's providers at the
  // moment it is called. Re-deriving on it is what changes this function's identity when a wire
  // registers, and so what re-runs the effects and callbacks that hold it.
  return useMemo(
    () => (hostId: string | null) =>
      hostId && (isReady?.(hostId) ?? true) ? registry.connectHost(hostId) : null,
    [registry, revision, isReady],
  );
}

/**
 * The connection to `hostId`, or `null` when no registered provider can reach it — including when
 * `hostId` is `null` because no host is selected yet, and when the host is not ready yet: a waiting
 * host is asked nothing. Memoised on `hostId` and the registry, so a connection's identity is as
 * stable as its routing.
 */
export function useHostConnection(hostId: string | null): HostConnection | null {
  const connect = useHostConnector();
  return useMemo(() => connect(hostId), [connect, hostId]);
}

/**
 * A client for `service` on `hostId`, or `null` when the host is unreachable or unselected. Call
 * sites must guard the null.
 */
export function useHostClient<S extends DescService>(service: S, hostId: string | null): Client<S> | null {
  const connection = useHostConnection(hostId);
  // The connection memoises the client per service, so this returns one instance for as long as the
  // routing holds — a consumer keying an effect on it does not tear its stream down on every render.
  return useMemo(() => connection?.clientFor(service) ?? null, [connection, service]);
}

/** The daemon selection; throws naming the port when no {@link ClientsProvider} wraps the caller. */
export function useSelectedDaemon(): SelectedDaemonState {
  return useClientsPort().selectedDaemon;
}

export function useDaemons(): DaemonHost[] {
  return useSelectedDaemon().daemons;
}

/** Names a host by its listed label, or the remembered label of the selected one, or its id. */
export function useHostLabel(): (instanceId: string) => string {
  const { daemons, selectedInstanceId, selectedLabel } = useSelectedDaemon();
  // Keyed on the content, not the array identities: the directory republishes a fresh list on every
  // change, and a callback that changed identity with it would re-run every effect holding it.
  const names = JSON.stringify([
    selectedInstanceId,
    selectedLabel,
    daemons.map((d) => [d.instanceId, d.label]),
  ]);
  return useMemo(() => {
    const [id, label, listed] = JSON.parse(names) as [string | null, string | null, [string, string][]];
    const source = {
      selectedInstanceId: id,
      selectedLabel: label,
      daemons: listed.map(([instanceId, l]) => ({ instanceId, label: l })),
    };
    return (instanceId: string) => hostLabelOf(source, instanceId);
  }, [names]);
}

/** A client for `service` on the daemon `instanceId`; `null` when it is unreachable or unselected. */
export function useDaemonClientFor<S extends DescService>(service: S, instanceId: string | null): Client<S> | null {
  return useHostClient(service, instanceId);
}

/** A client for `service` on the selected daemon; `null` until one is selected and reachable. */
export function useDaemonClient<S extends DescService>(service: S): Client<S> | null {
  const { selectedInstanceId } = useSelectedDaemon();
  return useDaemonClientFor(service, selectedInstanceId);
}
