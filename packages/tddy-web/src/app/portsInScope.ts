import type { LiveKitPorts } from "tddy-livekit-web";
import type { NavigationPort } from "tddy-client-web";
import type { AppPorts } from "./ports";

/**
 * The ports `AppClientsPorts` mounts from the app container. There is no fallback: outside a composed
 * app, or from a handler that left one out, the missing port is named.
 */
export function portsInScope(scope: AppPorts | null): { navigation: NavigationPort; liveKit: LiveKitPorts } {
  if (scope === null) {
    throw new Error(
      "AppClientsPorts needs the navigation and LiveKit ports: mount it under the app container (composeApp), or provide them through AppPortsScope",
    );
  }
  if (scope.roomRpc === undefined) {
    throw new Error("AppClientsPorts needs the roomRpc port: the AppPorts of the app handler lack it");
  }
  return { navigation: scope.navigation, liveKit: { presence: scope.presence, media: scope.media, roomRpc: scope.roomRpc } };
}
