/**
 * Where the desktop application offers its own wire to the app.
 *
 * `./localHost` knows what an IPC-reached host *is*; this knows *when* the page has one and hands it
 * to the registry node 1 built. They are separate files so the first stays free of React and can be
 * unit-tested as the plain factories it is.
 *
 * Deliberately the same shape as `LiveKitConnections` in `./liveKit`, down to why: a wire registers
 * itself while the component that offers it renders, holds its provider in a ref rather than a memo,
 * and re-provides the registry downwards. Read that component's doc comment for the reasoning —
 * repeating a registration pattern is what keeps precedence between the two wires legible, and
 * precedence is the whole of how the desktop's own host stays off the media server.
 *
 * Changeset: `docs/dev/1-WIP/2026-09-05-optional-livekit-desktop-ipc-host.md`.
 */

import { useRef, type ReactNode } from "react";
import { useAuthTokenGate, useTrafficMeterRegistry } from "../transportProvider";
import { createDefaultWebviewIpcTransport } from "../daemonTransport";
import { createIpcConnectionProvider, type LocalHostRegistration } from "./localHost";
import { ConnectionProviders, useConnectionProviders } from "./registry";
import type { ConnectionProvider } from "./types";

export interface LocalHostConnectionsProps {
  /**
   * The host this page's own application serves, from `localHostRegistrationFor`; `null` in a
   * browser, where there is no IPC bridge and the daemon is reached over HTTP.
   *
   * `null` is not a lesser case to be handled — it is the browser, and the answer for it is that
   * nothing is registered at all. The subtree renders identically either way, which is what makes
   * mounting this component unconditionally correct and leaves the flavour decision in one place.
   */
  registration: LocalHostRegistration | null;
  children: ReactNode;
}

/**
 * Register the desktop's own host as a connection provider for the subtree.
 *
 * Mount it **above** `SelectedDaemonProvider`, which is what registers the common room: precedence
 * is registration order, first match wins, and a parent renders before its children. That ordering
 * is the entire mechanism by which this page reaches its own daemon in-process even when a common
 * room is configured and could also reach that machine — there is no preference setting, and none is
 * wanted.
 *
 * The transport factory is assembled here rather than defaulted inside the provider because the two
 * things it needs are this subtree's: the traffic meter that makes the desktop's calls show up in
 * the same status bar a browser's do, and the auth gate that puts a request-time-fresh access token
 * on every call. A webview stays open longer than an access token lives, so a transport built
 * without the gate would send stale credentials — which is why `LocalHostWiring.transportFor` has no
 * default and this is the site that supplies it.
 */
export function LocalHostConnections({ registration, children }: LocalHostConnectionsProps) {
  const registry = useConnectionProviders();
  const meters = useTrafficMeterRegistry();
  const authTokenGate = useAuthTokenGate();

  // Both of those are held in refs by `RpcTransportProvider` and so are stable for as long as this
  // subtree exists — unlike the LiveKit transport factory, which is rebuilt every render and forced
  // its provider to grow a rebind. Capturing them once is therefore safe, and a provider that never
  // has to be replaced is one that never invalidates a client somebody keyed an effect on.
  const registered = useRef<{ hostId: string; provider: ConnectionProvider } | null>(null);
  if (registration && registered.current?.hostId !== registration.daemonInstanceId) {
    registered.current = {
      hostId: registration.daemonInstanceId,
      provider: createIpcConnectionProvider(registration, {
        transportFor: (bridge) =>
          createDefaultWebviewIpcTransport(bridge, meters ?? undefined, authTokenGate),
      }),
    };
  }
  // Unconditionally, on every render, exactly as `LiveKitConnections` does: `register` is idempotent
  // for the instance it already holds, and performing it from a `useMemo` would make the
  // registration depend on whether React kept the render.
  if (registration && registered.current) registry.register(registered.current.provider);

  return <ConnectionProviders registry={registry}>{children}</ConnectionProviders>;
}
