import "./index.css";
import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { createRoot } from "react-dom/client";
import type { Room } from "livekit-client";
import { RpcTransportProvider, useHttpClient, useHttpTransport } from "./rpc/transportProvider";
import { loadClientConfig } from "./rpc/clientConfig";
import { AuthProvider, useAuthContext } from "./hooks/authProvider";
import { SelectedDaemonProvider } from "./rpc/selectedDaemon";
import { ConnectionProviders } from "./rpc/connections/registry";
import type { TauriHostWindow } from "./rpc/daemonTransportFlavour";
import {
  createLocalHostDirectorySource,
  localHostRegistrationFor,
} from "./rpc/connections/localHost";
import { LocalHostConnections } from "./rpc/connections/localHostRegistration";
import type { DaemonHost } from "./lib/participantRole";
import { GhosttyTerminalSession } from "./components/GhosttyTerminalSession";
import { useDirectRoomTerminal } from "./rpc/connections/livekit/useDirectRoomTerminal";
import { ConnectionTerminalChrome } from "./components/connection/ConnectionTerminalChrome";
import { BUILD_ID } from "./buildId";

function HmrOverlay() {
  const [count, setCount] = useState(0);
  const meta = import.meta as { hot?: { on: (event: string, cb: () => void) => (() => void) | void } };
  const hot = meta.hot;
  useEffect(() => {
    if (!hot) return;
    const dispose = hot.on("vite:afterUpdate", () => setCount((c) => c + 1));
    return () => {
      if (typeof dispose === "function") dispose();
    };
  }, [hot]);
  if (!hot) return null;
  return (
    <span
      data-testid="hmr-count"
      style={{
        position: "fixed",
        bottom: 8,
        left: 8,
        fontSize: 10,
        color: "#888",
        zIndex: 9999,
        fontFamily: "monospace",
      }}
    >
      HMR: {count}
    </span>
  );
}

import { applyDebugMaskFromConfig, applyDebugMaskFromUrl } from "./lib/debugMask";
import { TokenService } from "./gen/token_pb";
import { useVisualViewport } from "./hooks/useVisualViewport";
import { GitHubLoginButton } from "./components/GitHubLoginButton";
import { AuthCallback } from "./components/AuthCallback";
import { UserAvatar } from "./components/UserAvatar";
import { Button } from "./components/ui/button";
import { LiveKitAppPage } from "./components/livekit/LiveKitAppPage";
import { WorktreesAppPage } from "./components/worktrees/WorktreesAppPage";
import { VmsAppPage } from "./components/vms/VmsAppPage";
import { ProjectsAppPage } from "./components/projects/ProjectsAppPage";
import { ModelsAppPage } from "./components/models/ModelsAppPage";
import { TasksDrawerScreen } from "./components/tasks/TasksDrawerScreen";
import { RpcPlaygroundAppPage } from "./rpc-playground/RpcPlaygroundAppPage";
import { SessionsDrawerScreen } from "./components/sessions/SessionsDrawerScreen";
import { SettingsAppPage } from "./components/settings/SettingsAppPage";
import {
  isRpcPlaygroundPath,
  isTasksPath,
  isVmsPath,
  isProjectsPath,
  isModelsPath,
  isLiveKitPath,
  isSettingsPath,
  isSessionsDrawerPath,
  parseTerminalSessionIdFromPathname,
  SESSIONS_DRAWER_ROUTE,
} from "./routing/appRoutes";
import { useAppLocation } from "./routing/useAppLocation";

function getParamsFromUrl(): { url: string; identity: string; roomName: string; debugLogging: boolean } {
  const params = typeof window !== "undefined" ? new URLSearchParams(window.location.search) : null;
  return {
    url: params?.get("url") ?? "",
    identity: params?.get("identity") ?? "",
    roomName: params?.get("roomName") ?? "terminal-e2e",
    debugLogging: params?.get("debug") === "1" || params?.get("debugLogging") === "1",
  };
}

function pushParamsToUrl(url: string, identity: string, roomName: string, debugLogging?: boolean): void {
  if (typeof window === "undefined") return;
  const params = new URLSearchParams();
  if (url) params.set("url", url);
  if (identity) params.set("identity", identity);
  if (roomName) params.set("roomName", roomName);
  if (debugLogging) params.set("debug", "1");
  const search = params.toString();
  const newUrl = search ? `${window.location.pathname}?${search}` : window.location.pathname;
  window.history.replaceState(null, "", newUrl);
}

const formClassName = "p-6 max-w-xl mx-auto font-sans text-foreground";
const inputClassName =
  "block w-full mb-3 px-2 py-2 text-sm rounded-md border border-input bg-background text-foreground box-border";
const labelClassName = "block mb-1 font-medium";

function ConnectedTerminal({
  url,
  identity,
  roomName,
  debugLogging,
  onDisconnect,
  onTerminate,
}: {
  url: string;
  identity: string;
  roomName: string;
  debugLogging?: boolean;
  onDisconnect: () => void;
  /** Standalone GitHub flow has no daemon session — omit Terminate. */
  onTerminate?: () => void;
}) {
  const client = useHttpClient(TokenService);
  const fullscreenTargetRef = useRef<HTMLDivElement>(null);
  const [initialToken, setInitialToken] = useState<string | null>(null);
  const [ttlSeconds, setTtlSeconds] = useState<bigint | null>(null);
  const [error, setError] = useState<string | null>(null);
  const { height: viewportHeight } = useVisualViewport();

  useEffect(() => {
    // `sessionToken` is not passed: the field exists on the request, so the transport's auth gate
    // fills it with a request-time-fresh access token (see `src/rpc/authGateInterceptor.ts`). The
    // daemon's registration of this mint refuses an unauthenticated caller; a session coder's own
    // `--web-port` registration does not.
    client
      .generateToken({ room: roomName, identity })
      .then((res) => {
        setInitialToken(res.token);
        setTtlSeconds(res.ttlSeconds);
      })
      .catch((e) => {
        setError(
          e instanceof Error
            ? e.message
            : "Token fetch failed. Ensure tddy-coder is running with --livekit-api-key and --livekit-api-secret."
        );
      });
  }, [client, roomName, identity]);

  const getToken = useMemo(
    () => async () => {
      const res = await client.refreshToken({ room: roomName, identity });
      return { token: res.token, ttlSeconds: res.ttlSeconds };
    },
    [client, roomName, identity]
  );

  // The room is this screen's to join: it was handed a url, an identity and a room name, and there
  // is no session and no daemon behind them to open a connection on. The terminal it feeds knows
  // none of that — see `useDirectRoomTerminal`.
  //
  // Called with the other hooks, above every early return. It used to be a component rendered in
  // the JSX below, where an early return was harmless; as a hook, returning before it would make
  // this render call fewer hooks than the last and React would throw instead of painting. The
  // failing render is precisely the `error` one below — so the screen whose job is to show a token
  // failure was the screen that could not.
  const terminal = useDirectRoomTerminal({
    url,
    token: initialToken ?? undefined,
    getToken,
    ttlSeconds: ttlSeconds ?? undefined,
    roomName,
    debug: debugLogging ?? false,
  });

  if (error) {
    return (
      <div className="p-6">
        <div data-testid="livekit-error">{error}</div>
      </div>
    );
  }
  const fullscreenContainerStyle: CSSProperties = {
    position: "fixed",
    top: 0,
    left: 0,
    right: 0,
    height: viewportHeight,
    margin: 0,
    overflow: "hidden",
    display: "flex",
    flexDirection: "column",
  };

  if (!initialToken || ttlSeconds === null) {
    return (
      <div ref={fullscreenTargetRef} data-testid="connected-terminal-container" style={fullscreenContainerStyle}>
        <div style={{ flex: 1, minHeight: 0, position: "relative" }}>
          <ConnectionTerminalChrome
            overlayStatus="connecting"
            buildId={BUILD_ID}
            onDisconnect={onDisconnect}
            onTerminate={onTerminate}
            fullscreenTargetRef={fullscreenTargetRef}
          />
        </div>
      </div>
    );
  }

  return (
    <div ref={fullscreenTargetRef} data-testid="connected-terminal-container" style={fullscreenContainerStyle}>
      {terminal.feed ? (
        <GhosttyTerminalSession
          feed={terminal.feed}
          connectionStatus={terminal.status}
          connectionError={terminal.error ?? undefined}
          debugLogging={debugLogging ?? false}
          connectionOverlay={{ onDisconnect, buildId: BUILD_ID, onTerminate }}
          fullscreenTargetRef={fullscreenTargetRef}
        />
      ) : (
        <div style={{ flex: 1, minHeight: 0, position: "relative" }}>
          <ConnectionTerminalChrome
            overlayStatus={terminal.status}
            buildId={BUILD_ID}
            onDisconnect={onDisconnect}
            onTerminate={onTerminate}
            fullscreenTargetRef={fullscreenTargetRef}
          />
        </div>
      )}
    </div>
  );
}

function ConnectionForm() {
  const { user, isAuthenticated, login, logout, error: authError } = useAuthContext();
  const [url, setUrl] = useState("");
  const [identity, setIdentity] = useState("");
  const [roomName, setRoomName] = useState("terminal-e2e");
  const [debugLogging, setDebugLogging] = useState(false);
  const [connected, setConnected] = useState(false);
  const transport = useHttpTransport();

  useEffect(() => {
    // URL params take priority, then server config, then defaults
    const params = getParamsFromUrl();

    loadClientConfig(transport)
      .then((config) => {
        applyDebugMaskFromConfig(config?.debug);
        setUrl(params.url || config?.livekitUrl || "");
        setIdentity(params.identity || "");
        setRoomName(params.roomName || config?.livekitRoom || "terminal-e2e");
        setDebugLogging(params.debugLogging);
      })
      .catch(() => {
        setUrl(params.url);
        setIdentity(params.identity);
        setRoomName(params.roomName || "terminal-e2e");
        setDebugLogging(params.debugLogging);
      });
  }, [transport]);

  if (connected && url && identity) {
    return (
      <ConnectedTerminal
        url={url}
        identity={identity}
        roomName={roomName}
        debugLogging={debugLogging}
        onDisconnect={() => setConnected(false)}
      />
    );
  }

  if (!isAuthenticated) {
    return (
      <div className={formClassName}>
        <h1>tddy-web</h1>
        <p className="mb-4 text-sm text-muted-foreground">
          Sign in with GitHub to access the terminal.
        </p>
        {authError ? (
          <p data-testid="auth-flow-error" className="mb-3 text-sm text-destructive">
            {authError}
          </p>
        ) : null}
        <GitHubLoginButton onClick={login} />
      </div>
    );
  }

  return (
    <div className={formClassName}>
      <h1>tddy-web</h1>
      {user && <UserAvatar user={user} onLogout={logout} />}
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (url && identity) {
            pushParamsToUrl(url, identity, roomName, debugLogging);
            setConnected(true);
          }
        }}
      >
        <label className={labelClassName} htmlFor="livekit-url">
          LiveKit URL
        </label>
        <input
          id="livekit-url"
          data-testid="livekit-url"
          type="text"
          placeholder="ws://192.168.1.10:7880"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          className={inputClassName}
        />
        <label className={labelClassName} htmlFor="livekit-identity">
          Identity
        </label>
        <input
          id="livekit-identity"
          data-testid="livekit-identity"
          type="text"
          placeholder="client"
          value={identity}
          onChange={(e) => setIdentity(e.target.value)}
          className={inputClassName}
        />
        <label className={labelClassName} htmlFor="livekit-room">
          Room name
        </label>
        <input
          id="livekit-room"
          data-testid="livekit-room"
          type="text"
          placeholder="terminal-e2e"
          value={roomName}
          onChange={(e) => setRoomName(e.target.value)}
          className={inputClassName}
        />
        <label className={`${labelClassName} flex items-center gap-2 mt-2`}>
          <input
            type="checkbox"
            checked={debugLogging}
            onChange={(e) => setDebugLogging(e.target.checked)}
          />
          Debug logging (mouse events, data flow)
        </label>
        <Button type="submit" disabled={!url || !identity}>
          Connect
        </Button>
      </form>
      <p className="mt-4 text-sm text-muted-foreground">
        Token is fetched from the server via Connect-RPC. Ensure tddy-coder is running with
        --livekit-api-key and --livekit-api-secret.
      </p>
    </div>
  );
}

function DaemonLoginScreen({ path, login, authError }: { path: string; login: (returnTo?: string) => void; authError: string | null }) {
  return (
    <div className={`${formClassName} flex flex-col gap-4 pt-12`}>
      <h1 className="text-2xl font-semibold m-0">Sign in</h1>
      <p className="text-sm text-muted-foreground m-0">
        Sign in with GitHub to continue to tddy-web.
      </p>
      {authError ? (
        <p data-testid="auth-flow-error" className="text-sm text-destructive m-0">
          {authError}
        </p>
      ) : null}
      <GitHubLoginButton onClick={() => login(path)} />
    </div>
  );
}

/**
 * Test-injection seam for `SelectedDaemonProvider`'s `room`/`daemons` overrides (mirrors
 * `RpcTransportProvider`'s `httpTransport`/`liveKitFactory` props) — `App` is the sole production
 * caller that constructs `SelectedDaemonProvider`, and does so only after its own async
 * `/api/config` fetch resolves, so a component test mounting `<App />` directly has no outer point
 * to inject a fake common-room connection unless `App` forwards these through itself. Both default
 * to `undefined`, so real usage (which never sets them) is unaffected.
 */
export interface AppProps {
  testDaemonRoom?: Room | null;
  testDaemonHosts?: DaemonHost[];
}

export function App({ testDaemonRoom, testDaemonHosts }: AppProps = {}) {
  const { location, navigate } = useAppLocation();
  const path = location.path;
  const { isAuthenticated, isLoading: authLoading, login, error: authError } = useAuthContext();
  const transport = useHttpTransport();
  const [appConfig, setAppConfig] = useState<{
    daemonMode: boolean | null;
    livekitUrl?: string;
    commonRoom?: string;
    daemonInstanceId?: string;
    allowedAgents?: { id: string; label: string }[];
  }>({ daemonMode: null });

  useEffect(() => {
    loadClientConfig(transport)
      .then((config) => {
        applyDebugMaskFromConfig(config?.debug);
        setAppConfig({
          daemonMode: config?.daemonMode ?? false,
          livekitUrl: config?.livekitUrl,
          commonRoom: config?.commonRoom,
          daemonInstanceId: config?.daemonInstanceId,
          allowedAgents: config?.allowedAgents,
        });
      })
      .catch(() => setAppConfig({ daemonMode: false }));
  }, [transport]);

  const daemonMode = appConfig.daemonMode;

  /**
   * The host this page's own application serves, when this page is running inside one.
   *
   * `null` in a browser, which is the whole of what keeps the IPC wire out of the browser's hands:
   * with no registration nothing is registered, and every host is reached exactly as it is today.
   * The question is `daemonTransportFlavour`'s, already asked to choose this page's own daemon
   * transport — asked once more here rather than re-asked in a second, differently-worded form.
   */
  const localHost = useMemo(
    () =>
      localHostRegistrationFor(
        typeof window === "undefined" ? {} : (window as TauriHostWindow),
        appConfig.daemonInstanceId,
      ),
    [appConfig.daemonInstanceId],
  );
  // Memoised on the registration rather than rebuilt per render: the array is the directory's source
  // list, and a new one every render would re-merge the directory and re-resolve the selected host.
  const localHostSources = useMemo(
    () => (localHost ? [createLocalHostDirectorySource(localHost)] : []),
    [localHost],
  );

  // Standalone mode uses query params for LiveKit fields, not `/terminal/:id`. Strip misleading hash paths.
  useEffect(() => {
    if (daemonMode !== false || typeof window === "undefined") return;
    if (parseTerminalSessionIdFromPathname(path) !== null) {
      navigate("/", { replace: true });
    }
  }, [daemonMode, path, navigate]);

  // `#/` and `#/sessions` render the same screen; canonicalise so a copied address bar names the
  // screen it is showing. `replace`: the operator did not navigate anywhere.
  useEffect(() => {
    if (daemonMode !== true || path !== "/") return;
    navigate(SESSIONS_DRAWER_ROUTE, { replace: true });
  }, [daemonMode, path, navigate]);

  return (
    <>
      {(typeof window !== "undefined" ? window.location.pathname : "/") === "/auth/callback" ? (
        <AuthCallback />
      ) : daemonMode === null || (daemonMode === true && authLoading) ? (
        <div className="p-6">Loading…</div>
      ) : daemonMode === true ? (
        !isAuthenticated ? (
          <DaemonLoginScreen path={path} login={login} authError={authError} />
        ) : (
          /* `LocalHostConnections` sits above `SelectedDaemonProvider`, which is what offers the
             common room: precedence is registration order and a parent renders first, so the
             desktop's own host stays on its in-process bridge even where a common room could also
             reach that machine. In a browser `localHost` is `null` and it registers nothing. */
          <LocalHostConnections registration={localHost}>
            <SelectedDaemonProvider
              livekitUrl={appConfig.livekitUrl}
              commonRoom={appConfig.commonRoom}
              servingInstanceId={appConfig.daemonInstanceId}
              room={testDaemonRoom}
              daemons={testDaemonHosts}
              hostSources={localHostSources}
            >
              {isRpcPlaygroundPath(path) ? (
                <RpcPlaygroundAppPage onNavigate={navigate} />
              ) : isTasksPath(path) ? (
                <TasksDrawerScreen onNavigate={navigate} />
              ) : isVmsPath(path) ? (
                <VmsAppPage onNavigate={navigate} />
              ) : isProjectsPath(path) ? (
                <ProjectsAppPage onNavigate={navigate} />
              ) : isModelsPath(path) ? (
                <ModelsAppPage onNavigate={navigate} />
              ) : isLiveKitPath(path) ? (
                <LiveKitAppPage onNavigate={navigate} />
              ) : isSettingsPath(path) ? (
                <SettingsAppPage onNavigate={navigate} />
              ) : path === "/worktrees" ? (
                <WorktreesAppPage onNavigate={navigate} />
              ) : isSessionsDrawerPath(path) ? (
                <SessionsDrawerScreen onNavigate={navigate} />
              ) : (
                <SessionsDrawerScreen onNavigate={navigate} />
              )}
            </SelectedDaemonProvider>
          </LocalHostConnections>
        )
      ) : (
        <ConnectionForm />
      )}
      <HmrOverlay />
    </>
  );
}

// Honour `?debug=` immediately (before terminals mount); `/api/config` re-syncs afterwards.
applyDebugMaskFromUrl();

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    // One registry for the page's whole lifetime. It is empty here: the wires register themselves
    // as they come up — the common room from `SelectedDaemonProvider`, and in a host build whatever
    // that build knows how to reach its own daemon over. A page where none of them does resolves
    // every host to `null`, which is the "not connected" state each screen already renders.
    <RpcTransportProvider>
      <ConnectionProviders>
        <AuthProvider>
          <App />
        </AuthProvider>
      </ConnectionProviders>
    </RpcTransportProvider>,
  );
}
