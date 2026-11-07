/**
 * Doubles for the ports, and `mountWithClients` — the Cypress mount every component package's specs
 * use instead of `tddy-web`'s providers: one in-memory RPC backend answers HTTP, daemon and host calls.
 */
import type { ReactElement } from "react";
import { createClient, type Transport } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ConnectionProviderRegistry } from "../connections/registry";
import type { ConnectionProvider, HostConnection } from "../connections/types";
import { VaultState } from "../gen/auth_pb";
import { formatAppLocation, parseAppLocation } from "../appLocation";
import { ClientsProvider, type ClientsPort, type DaemonHost } from "../ports/clients";
import { NavigationProvider, type AppLocation, type NavigationPort } from "../ports/navigation";
import { AuthPortProvider, type AuthPort } from "../ports/auth";
import { aSpy, type Spy } from "./spy";

export interface ClientsPortDoubleOptions {
  /** Hosts the double's directory lists; the first is selected. Default: one host, `host-a`. */
  daemons?: DaemonHost[];
  selectedInstanceId?: string | null;
}

const DEFAULT_DAEMONS: DaemonHost[] = [{ instanceId: "host-a", label: "host-a" }];

/** A host that answers every call from `transport`. Its clients are memoised per service. */
function aHostConnectionOver(hostId: string, transport: Transport): HostConnection {
  const clients = new Map<DescService, unknown>();
  return {
    hostId,
    providerId: "in-memory",
    status: "connected",
    error: null,
    capabilities: new Set(["rpc"]),
    clientFor: <S extends DescService>(service: S) => {
      if (!clients.has(service)) clients.set(service, createClient(service, transport));
      return clients.get(service) as ReturnType<typeof createClient<S>>;
    },
    transport: () => transport,
    openSession: () => {
      throw new Error("the in-memory host double serves RPC only; it opens no session attachments");
    },
  };
}

/** A clients port whose every transport (HTTP and every host) is `backend`'s. */
export function aClientsPortDouble(backend: InMemoryRpcBackend, options: ClientsPortDoubleOptions = {}): ClientsPort {
  const transport = backend.transport();
  const daemons = options.daemons ?? DEFAULT_DAEMONS;
  const listed = new Set(daemons.map((d) => d.instanceId));
  const connections = new ConnectionProviderRegistry();
  const byHost = new Map<string, HostConnection>();
  const provider: ConnectionProvider = {
    id: "in-memory",
    connectHost: (hostId) => {
      if (!listed.has(hostId)) return null;
      if (!byHost.has(hostId)) byHost.set(hostId, aHostConnectionOver(hostId, transport));
      return byHost.get(hostId) ?? null;
    },
  };
  connections.register(provider);
  const selectedInstanceId =
    options.selectedInstanceId !== undefined ? options.selectedInstanceId : (daemons[0]?.instanceId ?? null);
  return {
    httpTransport: transport,
    connections,
    selectedDaemon: {
      daemons,
      selectedInstanceId,
      selectedLabel: daemons.find((d) => d.instanceId === selectedInstanceId)?.label ?? null,
      selectDaemon: () => {},
      directoryStatus: "connected",
    },
  };
}

/** An in-memory navigation port starting at `initial` (default `/`). */
export function aNavigationDouble(initial: string | AppLocation = "/"): NavigationPort & { readonly history: AppLocation[] } {
  const asLocation = (to: string | AppLocation): AppLocation => (typeof to === "string" ? parseAppLocation(to) : to);
  let current = asLocation(initial);
  const history: AppLocation[] = [current];
  const listeners = new Set<() => void>();
  return {
    history,
    location: () => current,
    navigate: (to, options) => {
      const next = asLocation(to);
      // The round trip through the hash spelling is what normalises a location, as the app's own port does.
      current = parseAppLocation(formatAppLocation(next));
      if (options?.replace) history[history.length - 1] = current;
      else history.push(current);
      for (const listener of [...listeners]) listener();
    },
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}

type AuthAction = "login" | "handleCallback" | "logout" | "startDeviceLogin" | "unlockVault" | "resetVault";

/** An auth port whose every action is a {@link Spy}: it resolves and records its calls. */
export type AuthDouble = Omit<AuthPort, AuthAction> & {
  readonly [K in AuthAction]: Spy<Parameters<AuthPort[K]>, ReturnType<AuthPort[K]>>;
};

/** A signed-in (or, with state overrides, any) auth port whose actions resolve and record their calls. */
export function anAuthDouble(overrides: Partial<Omit<AuthPort, AuthAction>> = {}): AuthDouble {
  return {
    user: null,
    isAuthenticated: true,
    isLoading: false,
    error: null,
    sessionToken: "double-token",
    isRefreshing: false,
    vaultState: VaultState.OPEN,
    deviceLogin: { phase: "idle" },
    login: aSpy(async (_returnTo?: string) => {}),
    handleCallback: aSpy(async (_code: string, _state: string) => {}),
    logout: aSpy(async () => {}),
    startDeviceLogin: aSpy(async () => {}),
    unlockVault: aSpy(async (_passphrase: string, _create: boolean) => {}),
    resetVault: aSpy(async (_newPassphrase: string) => {}),
    ...overrides,
  };
}

export interface MountWithClientsOverrides {
  clients?: ClientsPort;
  navigation?: NavigationPort;
  auth?: AuthPort;
}

/** Wrap `element` in the three ports, backed by `backend` unless overridden. */
export function withClients(element: ReactElement, backend: InMemoryRpcBackend, overrides: MountWithClientsOverrides = {}): ReactElement {
  return (
    <ClientsProvider port={overrides.clients ?? aClientsPortDouble(backend)}>
      <NavigationProvider port={overrides.navigation ?? aNavigationDouble()}>
        <AuthPortProvider port={overrides.auth ?? anAuthDouble()}>{element}</AuthPortProvider>
      </NavigationProvider>
    </ClientsProvider>
  );
}

export { aPresenceDouble, aMediaDouble, aRoomRpcDouble, type PresenceDouble, type MediaDouble, type RoomRpcDouble } from "./media";
