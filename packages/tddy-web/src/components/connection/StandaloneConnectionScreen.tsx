/**
 * The **standalone** LiveKit connection screen — what `index.tsx` renders when `/api/config` says
 * this page is not served by a daemon (`daemonMode === false`).
 *
 * It is a page of its own: its LiveKit url, identity and room come from query parameters rather
 * than from a session, and the terminal it opens joins that room directly, with no daemon and no
 * session behind it. Lifted out of `index.tsx` whole, for that reason — nothing here is reachable
 * from the daemon-mode app, and nothing in the daemon-mode app is reachable from here.
 */

import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { useHttpClient, useHttpTransport } from "../../rpc/transportProvider";
import { loadClientConfig } from "../../rpc/clientConfig";
import { applyDebugMaskFromConfig } from "../../lib/debugMask";
import { useAuthContext } from "../../hooks/authProvider";
import { TokenService } from "../../gen/token_pb";
import { useVisualViewport } from "../../hooks/useVisualViewport";
import { GitHubLoginButton } from "../GitHubLoginButton";
import { UserAvatar } from "../UserAvatar";
import { Button } from "../ui/button";
import { GhosttyTerminalSession } from "../GhosttyTerminalSession";
import { useDirectRoomTerminal } from "../../rpc/connections/livekit/useDirectRoomTerminal";
import { ConnectionTerminalChrome } from "./ConnectionTerminalChrome";
import { BUILD_ID } from "../../buildId";
import { formClassName, inputClassName, labelClassName } from "./standaloneFormStyles";

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

export function ConnectionForm() {
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
