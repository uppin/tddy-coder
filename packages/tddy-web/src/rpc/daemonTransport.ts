/**
 * The transport this page reaches its own daemon with.
 *
 * One web bundle serves two hosts. In a browser the daemon is the HTTP server the bundle came
 * from, so calls go to `{origin}/rpc`. Inside the Tauri desktop application the daemon runs in the
 * same process and is reached over the host application's IPC bridge — there is no origin to post
 * to. Which of the two applies is decided at runtime by {@link daemonTransportFlavour}, so there is
 * one build and the browser dashboard cannot silently diverge from the desktop one.
 *
 * Both flavours carry the **same** interceptor stack: traffic metering, and the auth gate that
 * refreshes `sessionToken` per request. A desktop operator losing the gate would send stale tokens
 * from a webview that has been open longer than an access token lives.
 */

import { createConnectTransport } from "@connectrpc/connect-web";
import type { Interceptor, Transport } from "@connectrpc/connect";
import { createTauriIpcBridge, createTauriTransport, type WebviewIpcBridge } from "tddy-tauri-web";
import { daemonTransportFlavour, type TauriHostWindow } from "./daemonTransportFlavour";
import { transportWithInterceptors } from "./interceptedTransport";
import { createTrafficInterceptor } from "./httpTrafficInterceptor";
import { createAuthGateInterceptor } from "./authGateInterceptor";
import type { TrafficMeterRegistry } from "./trafficMeter";

/**
 * Settable holder for the app's access-token resolver. The production transport's auth-gate
 * interceptor consults `current` per request; `AuthProvider` installs the real single-flight
 * resolver once mounted. While `current` is `null` (before mount, or with no auth provider), the
 * gate injects nothing and leaves each request's own `sessionToken` untouched.
 */
export type AuthTokenGate = { current: (() => Promise<string | null>) | null };

/**
 * Registry key for the meter that measures this page's control-plane RPC. It names the control
 * plane, not the wire: the desktop app's calls travel over IPC and are still counted here, so the
 * status bar reads the same in both hosts.
 */
const CONTROL_PLANE_METER = "http";

/** The `url` an interceptor sees for a call that reaches the daemon over the host's IPC bridge. */
const WEBVIEW_IPC_BASE_URL = "webview-ipc://daemon";

/** The parts of the page's `window` the transport choice depends on. */
export interface DaemonHostWindow extends TauriHostWindow {
  location?: { origin: string };
}

/**
 * The host this page is running in: the window that names the flavour, and the way to open the
 * host application's IPC bridge when that flavour is the webview one.
 */
export interface DaemonHostEnvironment {
  window: DaemonHostWindow;
  /** Called only on the webview-IPC path, so a browser page never touches the Tauri API. */
  createIpcBridge: () => WebviewIpcBridge;
}

/** The real host: this page's own `window`, and the host application's real bridge. */
export function thisPagesHost(): DaemonHostEnvironment {
  return {
    window: typeof window === "undefined" ? {} : (window as DaemonHostWindow),
    createIpcBridge: createTauriIpcBridge,
  };
}

/**
 * The interceptor stack every daemon transport carries.
 *
 * The auth gate runs outermost so recorded/sent bytes reflect the refreshed token. A `null`
 * resolver (no auth provider yet) leaves the request's own token in place.
 */
function daemonInterceptors(
  registry?: TrafficMeterRegistry,
  authTokenGate?: AuthTokenGate,
): Interceptor[] {
  const interceptors: Interceptor[] = [];
  if (authTokenGate) {
    interceptors.push(
      createAuthGateInterceptor(() =>
        authTokenGate.current ? authTokenGate.current() : Promise.resolve(null),
      ),
    );
  }
  if (registry) {
    interceptors.push(createTrafficInterceptor(registry.get(CONTROL_PLANE_METER)));
  }
  return interceptors;
}

/**
 * Factory for the production HTTP transport (binary Connect protocol, same-origin /rpc).
 * When a registry is provided, attaches a traffic-metering interceptor. Exported so a test can
 * point a `liveKitFactory` override at the same HTTP transport its `cy.intercept`s already expect,
 * without needing a real (or fully faked) LiveKit data-channel connection.
 */
export function createDefaultHttpTransport(
  registry?: TrafficMeterRegistry,
  authTokenGate?: AuthTokenGate,
  /** The page's own origin. Omitted outside a browser, where there is no origin to post to. */
  origin: string | undefined = typeof window !== "undefined" ? window.location.origin : undefined,
): Transport {
  return createConnectTransport({
    baseUrl: origin === undefined ? "" : `${origin}/rpc`,
    useBinaryFormat: true,
    interceptors: daemonInterceptors(registry, authTokenGate),
  });
}

/**
 * Factory for the production webview-IPC transport: envelope frames across the host application's
 * two IPC commands, carrying the same interceptor stack the HTTP transport does.
 */
export function createDefaultWebviewIpcTransport(
  bridge: WebviewIpcBridge,
  registry?: TrafficMeterRegistry,
  authTokenGate?: AuthTokenGate,
): Transport {
  return transportWithInterceptors({
    inner: createTauriTransport({ bridge }),
    interceptors: daemonInterceptors(registry, authTokenGate),
    baseUrl: WEBVIEW_IPC_BASE_URL,
  });
}

/**
 * The transport for the daemon serving this page, in whichever host the page is running.
 *
 * `host` is the injection seam: production passes nothing and the real `window` decides.
 */
export function createDefaultDaemonTransport(
  registry?: TrafficMeterRegistry,
  authTokenGate?: AuthTokenGate,
  host: DaemonHostEnvironment = thisPagesHost(),
): Transport {
  return daemonTransportFlavour(host.window) === "webview-ipc"
    ? createDefaultWebviewIpcTransport(host.createIpcBridge(), registry, authTokenGate)
    : createDefaultHttpTransport(registry, authTokenGate, host.window.location?.origin);
}
