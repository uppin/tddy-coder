/**
 * Doubles for the ports, and `mountWithClients` — the Cypress mount every component package's specs
 * use instead of `tddy-web`'s providers: one in-memory RPC backend answers HTTP, daemon and host calls.
 */
import type { ReactElement } from "react";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import type { ClientsPort, DaemonHost } from "../ports/clients";
import type { AppLocation, NavigationPort } from "../ports/navigation";
import type { AuthPort } from "../ports/auth";

export interface ClientsPortDoubleOptions {
  /** Hosts the double's directory lists; the first is selected. Default: one host, `host-a`. */
  daemons?: DaemonHost[];
  selectedInstanceId?: string | null;
}

/** A clients port whose every transport (HTTP and every host) is `backend`'s. */
export function aClientsPortDouble(backend: InMemoryRpcBackend, options: ClientsPortDoubleOptions = {}): ClientsPort {
  void backend;
  void options;
  throw new Error("TODO(clients): implement");
}

/** An in-memory navigation port starting at `initial` (default `/`). */
export function aNavigationDouble(initial?: string | AppLocation): NavigationPort & { readonly history: AppLocation[] } {
  void initial;
  throw new Error("TODO(clients): implement");
}

/** A signed-in (or, with overrides, any) auth port whose actions resolve and record their calls. */
export function anAuthDouble(overrides: Partial<AuthPort> = {}): AuthPort {
  void overrides;
  throw new Error("TODO(clients): implement");
}

export interface MountWithClientsOverrides {
  clients?: ClientsPort;
  navigation?: NavigationPort;
  auth?: AuthPort;
}

/** Wrap `element` in the three ports, backed by `backend` unless overridden. */
export function withClients(element: ReactElement, backend: InMemoryRpcBackend, overrides: MountWithClientsOverrides = {}): ReactElement {
  void element;
  void backend;
  void overrides;
  throw new Error("TODO(clients): implement");
}
