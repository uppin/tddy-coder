/**
 * Mounts the ports `tddy-client-web` defines, from the sources this app owns: its serving-daemon
 * transport, its connection registry, its selected-daemon directory and its URL-hash navigation.
 *
 * Every provider that changes one of those sources renders this below itself, so the ports in scope
 * for any subtree are always built from the nearest source of each kind — a spec that mounts only
 * `RpcTransportProvider`, or only `ConnectionProviders`, still gets a coherent port. With no
 * `SelectedDaemonProvider` in scope the directory source honestly reports "no daemons".
 *
 * It also mounts the LiveKit ports (`tddy-livekit-web`'s `livekitPorts`): presence and room-addressed
 * RPC, for every subtree that has a source of any kind above it.
 *
 * The slots the session screens ask the app for are filled here too: the screen shell (`AppShell`) and
 * the Hosts screen's passphrase dialog for a clone-blocked session start.
 */

import { useContext, useMemo, type ReactNode } from "react";
import { StartPassphraseProvider } from "tddy-session-create-web";
import { SessionShellProvider } from "tddy-session-web";
import { ClientsProvider, NavigationProvider, type ClientsPort } from "tddy-client-web";
import { LiveKitPortsProvider, livekitPorts } from "tddy-livekit-web";
import { HostStartPassphrasePrompt } from "../components/hosts/HostStartPassphrasePrompt";
import { AppShell } from "../components/shell/AppShell";
import { hashNavigationPort } from "../routing/useAppLocation";
import { HostReadinessContext, useConnectionProviders } from "./connections/registry";
import { useSelectedDaemonSource } from "./selectedDaemon";
import { useServingHttpTransport } from "./transportProvider";

/** The LiveKit presence and room-RPC ports, built once for the app's lifetime (their hooks must keep one identity). */
const liveKit = livekitPorts();

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
      <NavigationProvider port={hashNavigationPort}>
        <LiveKitPortsProvider ports={liveKit}>
          <SessionShellProvider shell={AppShell}>
            <StartPassphraseProvider prompt={HostStartPassphrasePrompt}>{children}</StartPassphraseProvider>
          </SessionShellProvider>
        </LiveKitPortsProvider>
      </NavigationProvider>
    </ClientsProvider>
  );
}
