/**
 * LiveKit OAuth relay: watch `daemon-*` metadata, open authorize URL, tunnel callback TCP.
 * The desktop process does not join LiveKit here — callers supply `startOAuthTcpTunnel` (tests use
 * stubs; production uses **tddy-daemon** for loopback TCP + `LoopbackTunnelService`).
 */
import {
  parseCodexOAuthMetadata,
  resolvedCodexOAuthCallbackPort,
} from "./codex-oauth-metadata";

const ROOM_EVENT_PARTICIPANT_CONNECTED = "participantConnected" as const;
const ROOM_EVENT_PARTICIPANT_METADATA_CHANGED = "participantMetadataChanged" as const;

/** Minimal participant shape for metadata-driven OAuth (matches LiveKit / mocks). */
export type OAuthRelayParticipant = {
  identity: string;
  metadata?: string | null;
};

/** Minimal room surface: event subscription + daemon participant iteration. */
export type OAuthRelayRoom = {
  remoteParticipants: { values(): IterableIterator<OAuthRelayParticipant> };
  on(event: string, handler: () => void): void;
};

function pickDaemonParticipant(room: OAuthRelayRoom): OAuthRelayParticipant | null {
  for (const p of room.remoteParticipants.values()) {
    if (p.identity.startsWith("daemon-")) {
      return p;
    }
  }
  return null;
}

/** Injected tunnel (daemon-backed in production; HTTP stub in tests). */
export type StartOAuthTcpTunnelFn<R extends OAuthRelayRoom = OAuthRelayRoom> = (opts: {
  room: R;
  targetIdentity: string;
  listenPort: number;
  remoteLoopbackPort: number;
  expectedState: string;
}) => { stop: () => void };

export type LiveKitOAuthRelayDeps<R extends OAuthRelayRoom = OAuthRelayRoom> = {
  openUrlInBrowser: (url: string) => void;
  startOAuthTcpTunnel: StartOAuthTcpTunnelFn<R>;
};

export type LiveKitOAuthRelayHandle = {
  dispose: () => void;
  /** Local TCP listen port when pending OAuth is active; otherwise `null`. */
  getCallbackPort: () => number | null;
};

/**
 * Watch daemon participant metadata, open authorize URL, start tunnel for OAuth callback.
 */
export async function installLiveKitOAuthRelay<R extends OAuthRelayRoom>(
  room: R,
  deps: LiveKitOAuthRelayDeps<R>,
): Promise<LiveKitOAuthRelayHandle> {
  const { openUrlInBrowser: openUrl, startOAuthTcpTunnel: startTunnel } = deps;

  let tunnel: { stop: () => void } | null = null;
  let listeningPort: number | null = null;
  let lastAuthorize: string | null = null;
  let callbackListenReserved = false;

  const stopTunnel = () => {
    if (tunnel) {
      tunnel.stop();
      tunnel = null;
    }
    listeningPort = null;
    callbackListenReserved = false;
  };

  const onMetadata = async () => {
    const daemon = pickDaemonParticipant(room);
    if (!daemon) return;
    const info = parseCodexOAuthMetadata(daemon.metadata ?? "");
    if (!info?.pending || !info.authorizeUrl) {
      stopTunnel();
      lastAuthorize = null;
      return;
    }
    if (info.authorizeUrl !== lastAuthorize) {
      lastAuthorize = info.authorizeUrl;
      openUrl(info.authorizeUrl);
    }
    const port = resolvedCodexOAuthCallbackPort(info);
    const expectedState = info.state ?? "";
    if (tunnel || callbackListenReserved) {
      return;
    }
    callbackListenReserved = true;
    const targetIdentity = daemon.identity;
    console.info(
      `[tddy-desktop] OAuth loopback TCP listening on 127.0.0.1:${port} (tunnel → ${targetIdentity} @ 127.0.0.1:${port})`,
    );
    try {
      listeningPort = port;
      tunnel = startTunnel({
        room,
        targetIdentity,
        listenPort: port,
        remoteLoopbackPort: port,
        expectedState,
      });
    } catch (e) {
      callbackListenReserved = false;
      listeningPort = null;
      console.error(
        "[tddy-desktop] OAuth relay: failed to bind loopback TCP listener:",
        e,
      );
      throw e;
    }
  };

  room.on(ROOM_EVENT_PARTICIPANT_CONNECTED, () => {
    void onMetadata();
  });
  room.on(ROOM_EVENT_PARTICIPANT_METADATA_CHANGED, () => {
    void onMetadata();
  });

  await onMetadata();

  return {
    dispose: () => {
      stopTunnel();
    },
    getCallbackPort: () => listeningPort,
  };
}
