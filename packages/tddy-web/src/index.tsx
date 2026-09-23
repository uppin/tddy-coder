import "./index.css";
import { useCallback, useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import type { Room } from "livekit-client";
import { RpcTransportProvider, useHttpTransport } from "./rpc/transportProvider";
import { loadClientConfig, type AuthFlowDeclaration } from "./rpc/clientConfig";
import { AuthProvider, useAuthContext } from "./hooks/authProvider";
import { SelectedDaemonProvider } from "./rpc/selectedDaemon";
import { ConnectionProviders } from "./rpc/connections/registry";
import type { TauriHostWindow } from "./rpc/daemonTransportFlavour";
import { localHostRegistrationFor } from "./rpc/connections/localHost";
import { LocalHostConnections } from "./rpc/connections/localHostRegistration";
import type { DaemonHost } from "./lib/participantRole";

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
import { DaemonLoginScreen } from "./components/DaemonLoginScreen";
import { CredentialVaultPrompt } from "./components/CredentialVaultPrompt";
import { AuthCallback } from "./components/AuthCallback";
import { LiveKitAppPage } from "./components/livekit/LiveKitAppPage";
import { WorktreesAppPage } from "./components/worktrees/WorktreesAppPage";
import { VmsAppPage } from "./components/vms/VmsAppPage";
import { ProjectsAppPage } from "./components/projects/ProjectsAppPage";
import { HostsAppPage } from "./components/hosts/HostsAppPage";
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
  isHostsPath,
  isModelsPath,
  isLiveKitPath,
  isSettingsPath,
  isSessionsDrawerPath,
  parseTerminalSessionIdFromPathname,
  SESSIONS_DRAWER_ROUTE,
} from "./routing/appRoutes";
import { useAppLocation } from "./routing/useAppLocation";
import { ConnectionForm } from "./components/connection/StandaloneConnectionScreen";

/** What the page read about the daemon serving it. */
interface ServingDaemonConfig {
  livekitEnabled?: boolean;
  livekitUrl?: string;
  commonRoom?: string;
  daemonInstanceId?: string;
  allowedAgents?: { id: string; label: string }[];
  sandboxedCodebase?: { confinesFilesystem: boolean };
}

/**
 * `daemonMode: null` is still loading. A daemon-mode page always carries the sign-in its daemon
 * declared, so the sign-in screen is never rendered from a guessed flow.
 */
type AppConfigState =
  | ({ daemonMode: null | false } & ServingDaemonConfig)
  | ({ daemonMode: true; authFlow: AuthFlowDeclaration } & ServingDaemonConfig);

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
  const [appConfig, setAppConfig] = useState<AppConfigState>({ daemonMode: null });

  useEffect(() => {
    loadClientConfig(transport)
      .then((config) => {
        applyDebugMaskFromConfig(config?.debug);
        const read = {
          livekitEnabled: config?.livekitEnabled,
          livekitUrl: config?.livekitUrl,
          commonRoom: config?.commonRoom,
          daemonInstanceId: config?.daemonInstanceId,
          allowedAgents: config?.allowedAgents,
          // The serving daemon's own jail capability. Dropped here it would reach no host
          // descriptor, and the sandboxed-codebase control would stay disabled on every daemon
          // that does not join a common room.
          sandboxedCodebase: config?.sandboxedCodebase,
        };
        setAppConfig(
          config?.daemonMode === true
            ? { ...read, daemonMode: true, authFlow: config.authFlow }
            : { ...read, daemonMode: false },
        );
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
      ) : appConfig.daemonMode === true ? (
        !isAuthenticated ? (
          <DaemonLoginScreen path={path} login={login} authError={authError} authFlow={appConfig.authFlow} />
        ) : (
          /* `LocalHostConnections` sits above `SelectedDaemonProvider`, which is what offers the
             common room: precedence is registration order and a parent renders first, so the
             desktop's own host stays on its in-process bridge even where a common room could also
             reach that machine. In a browser `localHost` is `null` and it registers nothing. */
          <LocalHostConnections registration={localHost}>
            <SelectedDaemonProvider
              livekitEnabled={appConfig.livekitEnabled}
              livekitUrl={appConfig.livekitUrl}
              commonRoom={appConfig.commonRoom}
              servingInstanceId={appConfig.daemonInstanceId}
              servingSandboxedCodebase={appConfig.sandboxedCodebase}
              room={testDaemonRoom}
              daemons={testDaemonHosts}
            >
              {isRpcPlaygroundPath(path) ? (
                <RpcPlaygroundAppPage onNavigate={navigate} />
              ) : isTasksPath(path) ? (
                <TasksDrawerScreen onNavigate={navigate} />
              ) : isVmsPath(path) ? (
                <VmsAppPage onNavigate={navigate} />
              ) : isProjectsPath(path) ? (
                <ProjectsAppPage onNavigate={navigate} />
              ) : isHostsPath(path) ? (
                <HostsAppPage onNavigate={navigate} />
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
      {/* Over whichever screen is showing: a signed-in operator whose vault is locked or not created
          is asked for its passphrase. It renders nothing otherwise. */}
      {daemonMode === true ? <CredentialVaultPrompt /> : null}
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
