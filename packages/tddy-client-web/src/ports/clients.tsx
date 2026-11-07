/**
 * The clients port: how a component package reaches the daemon, a host, or plain HTTP — without
 * knowing which wire (HTTP, Tauri IPC, LiveKit) the app chose. The app supplies a {@link ClientsPort}
 * through {@link ClientsProvider}; the hooks below read it. The hook names are the ones `tddy-web`
 * used before the split, so a moved component changes only its import path.
 */
import type { ReactElement, ReactNode } from "react";
import type { Client, Transport } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import type { ConnectionStatus, HostConnection, ConnectionProvider } from "../connections/types";

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

/** Where connection providers register; the hooks resolve a host through it. */
export interface ConnectionRegistry {
  register(provider: ConnectionProvider): () => void;
  connectHost(hostId: string): HostConnection | null;
  subscribe(listener: () => void): () => void;
  revision(): number;
}

/** Everything the app supplies so component packages can build clients. */
export interface ClientsPort {
  /** The transport for services the serving daemon answers over plain HTTP (auth, config). */
  readonly httpTransport: Transport;
  /** The provider registry hosts are reached through. */
  readonly connections: ConnectionRegistry;
  /** Whether `hostId` may be dialled yet (e.g. its room has joined). Every host is ready when absent. */
  readonly isHostReady?: (hostId: string) => boolean;
  /** The current daemon selection. */
  readonly selectedDaemon: SelectedDaemonState;
}

export function ClientsProvider(props: { port: ClientsPort; children?: ReactNode }): ReactElement {
  void props;
  throw new Error("TODO(clients): implement");
}

/** The port in scope; throws naming the port when no {@link ClientsProvider} wraps the caller. */
export function useClientsPort(): ClientsPort {
  throw new Error("TODO(clients): implement");
}

export function useHttpTransport(): Transport {
  throw new Error("TODO(clients): implement");
}

export function useHttpClient<S extends DescService>(service: S): Client<S> {
  void service;
  throw new Error("TODO(clients): implement");
}

export function useConnectionRegistry(): ConnectionRegistry {
  throw new Error("TODO(clients): implement");
}

export function useHostConnector(): (hostId: string | null) => HostConnection | null {
  throw new Error("TODO(clients): implement");
}

export function useHostConnection(hostId: string | null): HostConnection | null {
  void hostId;
  throw new Error("TODO(clients): implement");
}

export function useHostClient<S extends DescService>(service: S, hostId: string | null): Client<S> | null {
  void service;
  void hostId;
  throw new Error("TODO(clients): implement");
}

export function useSelectedDaemon(): SelectedDaemonState {
  throw new Error("TODO(clients): implement");
}

export function useDaemons(): DaemonHost[] {
  throw new Error("TODO(clients): implement");
}

export function useHostLabel(): (instanceId: string) => string {
  throw new Error("TODO(clients): implement");
}

export function useDaemonClientFor<S extends DescService>(service: S, instanceId: string | null): Client<S> | null {
  void service;
  void instanceId;
  throw new Error("TODO(clients): implement");
}

export function useDaemonClient<S extends DescService>(service: S): Client<S> | null {
  void service;
  throw new Error("TODO(clients): implement");
}
