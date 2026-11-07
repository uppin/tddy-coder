/**
 * The app as a composition of the web packages: one app container (tddy-react-commons) whose handler
 * starts the port implementations — URL navigation, the LiveKit presence and media ports, the
 * window's native file drop — and whose tree mounts the shell and routes over every package's screens.
 *
 * The clients and auth ports are built by the stateful providers in the tree (`RpcTransportProvider`,
 * `ConnectionProviders`, `AuthProvider`, `SelectedDaemonProvider`) — see {@link AppPorts}; a handler
 * that supplies them (the specs' doubles) takes those providers' place.
 */
import type { ReactElement } from "react";
import { ReactAppContainer, useAppContext, type AppHandler } from "tddy-react-commons/container";
import { ClientsProvider } from "tddy-client-web";
import { daemonTransportFlavour, type TauriHostWindow } from "tddy-session-web";
import { livekitPorts } from "tddy-livekit-web";
import { thisWindowsNativeFileDrop } from "tddy-tauri-web";
import { App } from "./App";
import { AuthFromPort, AuthProvider } from "./auth/authProvider";
import { AppPortSlots } from "./rpc/appClientsPorts";
import { ConnectionProviders } from "./rpc/connections/registry";
import { RpcTransportProvider } from "./rpc/transportProvider";
import { AppPortsScope, type AppPorts } from "./ports";
import { hashNavigationPort } from "../routing/useAppLocation";

export type { AppPorts } from "./ports";

/** Where the app runs: the window whose host (browser or application) decides how it is reached. */
export interface AppEnvironment {
  readonly window: TauriHostWindow | undefined;
}

/** The handler that builds the ports from `environment` and tears them down with the app. */
export function appHandler(environment: AppEnvironment): AppHandler<AppPorts> {
  return {
    start: () => {
      const liveKit = livekitPorts();
      const ports: AppPorts = {
        navigation: hashNavigationPort,
        presence: liveKit.presence,
        media: liveKit.media,
        roomRpc: liveKit.roomRpc,
        nativeFileDrop:
          environment.window !== undefined && daemonTransportFlavour(environment.window) === "webview-ipc"
            ? thisWindowsNativeFileDrop()
            : null,
      };
      return { context: ports, teardown: () => {} };
    },
  };
}

/** The provider stack under the container: the handler's ports, then the sources that are still React state. */
function AppTree(): ReactElement {
  const ports = useAppContext<AppPorts>();
  const { clients, auth } = ports;
  if ((clients === undefined) !== (auth === undefined)) {
    throw new Error("appHandler ports: `clients` and `auth` are supplied together or not at all");
  }
  if (clients && auth) {
    return (
      <AppPortsScope.Provider value={ports}>
        <ClientsProvider port={clients}>
          <AuthFromPort port={auth}>
            <AppPortSlots>
              <App />
            </AppPortSlots>
          </AuthFromPort>
        </ClientsProvider>
      </AppPortsScope.Provider>
    );
  }
  return (
    <AppPortsScope.Provider value={ports}>
      {/* One registry for the page's whole lifetime. It is empty here: the wires register themselves
          as they come up — the common room from `SelectedDaemonProvider`, and in a host build whatever
          that build knows how to reach its own daemon over. */}
      <RpcTransportProvider>
        <ConnectionProviders>
          {/* AuthProvider mounts AppClientsPorts, whose AppPortSlots also supply the session shell,
              the traffic seam and the Hosts screen's start-passphrase prompt. */}
          <AuthProvider>
            <App />
          </AuthProvider>
        </ConnectionProviders>
      </RpcTransportProvider>
    </AppPortsScope.Provider>
  );
}

/** The whole app: the container over the shell and routes. */
export function composeApp(handler: AppHandler<AppPorts>): ReactElement {
  return ReactAppContainer.create(handler, <AppTree />);
}
