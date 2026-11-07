/**
 * The app as a composition of the web packages: one app container (tddy-react-commons) whose handler
 * starts the port implementations — HTTP/Tauri transports, the LiveKit ports, URL navigation, auth —
 * and whose tree mounts the shell and routes over every package's screens.
 */
import type { ReactElement } from "react";
import type { AppHandler } from "tddy-react-commons/container";
import type { AuthPort, ClientsPort, MediaPort, NavigationPort, PresencePort } from "tddy-client-web";

/** What the app hands every package: the ports, built once for the app's lifetime. */
export interface AppPorts {
  readonly clients: ClientsPort;
  readonly navigation: NavigationPort;
  readonly auth: AuthPort;
  readonly presence: PresencePort;
  readonly media: MediaPort;
}

/** Where the app's configuration comes from (`/api/config`, the desktop host, the build). */
export interface AppEnvironment {
  readonly origin: string;
  readonly livekitEnabled: boolean;
  readonly livekitUrl?: string;
  readonly commonRoom?: string;
  readonly servingInstanceId?: string;
}

/** The handler that builds the ports from `environment` and tears them down with the app. */
export function appHandler(environment: AppEnvironment): AppHandler<AppPorts> {
  void environment;
  throw new Error("TODO(wiring): implement");
}

/** The whole app: the container over the shell and routes. */
export function composeApp(handler: AppHandler<AppPorts>): ReactElement {
  void handler;
  throw new Error("TODO(wiring): implement");
}
