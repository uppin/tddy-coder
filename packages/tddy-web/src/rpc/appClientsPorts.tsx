/**
 * Mounts the ports `tddy-client-web` defines, from the sources this app owns: its serving-daemon
 * transport, its connection registry, its selected-daemon directory and its URL-hash navigation.
 *
 * Every provider that changes one of those sources renders this below itself, so the ports in scope
 * for any subtree are always built from the nearest source of each kind — a spec that mounts only
 * `RpcTransportProvider`, or only `ConnectionProviders`, still gets a coherent port. With no
 * `SelectedDaemonProvider` in scope the directory source honestly reports "no daemons".
 */

import { useContext, useMemo, type ReactNode } from "react";
import { ClientsProvider, NavigationProvider, type ClientsPort } from "tddy-client-web";
import { hashNavigationPort } from "../routing/useAppLocation";
import { HostReadinessContext, useConnectionProviders } from "./connections/registry";
import { useSelectedDaemonSource } from "./selectedDaemon";
import { useServingHttpTransport } from "./transportProvider";

export function AppClientsPorts({ children }: { children: ReactNode }) {
  const httpTransport = useServingHttpTransport();
  const connections = useConnectionProviders();
  const selectedDaemon = useSelectedDaemonSource();
  const isHostReady = useContext(HostReadinessContext) ?? undefined;
  const port: ClientsPort = useMemo(
    () => ({ httpTransport, connections, selectedDaemon, isHostReady }),
    [httpTransport, connections, selectedDaemon, isHostReady],
  );
  return (
    <ClientsProvider port={port}>
      <NavigationProvider port={hashNavigationPort}>{children}</NavigationProvider>
    </ClientsProvider>
  );
}
