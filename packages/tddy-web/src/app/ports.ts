/**
 * What the app container hands the tree: the ports the handler built once for the app's lifetime,
 * and the context that offers them to the providers below that still own the stateful sources.
 */
import { createContext } from "react";
import type { AuthPort, ClientsPort, MediaPort, NavigationPort, PresencePort } from "tddy-client-web";
import type { LiveKitPorts } from "tddy-livekit-web";
import type { NativeFileDropBridge } from "tddy-tauri-web";

export interface AppPorts {
  /**
   * The clients port. A handler that supplies it (the specs' doubles) replaces the in-tree sources
   * that build it today — the HTTP/Tauri transport, connection registry and selected-daemon directory,
   * which are React state and cannot be built outside the tree yet. Absent in production.
   * TODO(wiring): build it in `appHandler` once those sources are React-free.
   */
  readonly clients?: ClientsPort;
  /** The auth port; same story as {@link AppPorts.clients} (`useAuth` is a hook today). */
  readonly auth?: AuthPort;
  readonly navigation: NavigationPort;
  readonly presence: PresencePort;
  readonly media: MediaPort;
  /** Room-addressed RPC. */
  readonly roomRpc: LiveKitPorts["roomRpc"];
  /** The application window's drag-drop surface, `null` in a browser. */
  readonly nativeFileDrop?: NativeFileDropBridge | null;
}

/**
 * The ports in scope for the stateful providers (`AppClientsPorts`). `null` outside a composed app —
 * a spec mounting one provider alone — where each falls back to the app's own default.
 */
export const AppPortsScope = createContext<AppPorts | null>(null);
