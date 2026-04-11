import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import {
  Room,
  RoomEvent,
  type RemoteParticipant,
} from "@livekit/rtc-node";

import {
  parseCodexOAuthMetadata,
  resolvedCodexOAuthCallbackPort,
} from "./codex-oauth-metadata";
import { startOAuthLoopbackTcpProxy } from "./oauth-loopback-tcp-proxy";
import { TokenService } from "../../../tddy-web/src/gen/token_pb.ts";

function openUrlInBrowser(url: string) {
  const platform = process.platform;
  const cmd =
    platform === "darwin"
      ? ["open", url]
      : platform === "win32"
        ? ["cmd", "/c", "start", "", url]
        : ["xdg-open", url];
  Bun.spawn(cmd, { stdout: "ignore", stderr: "ignore" });
}

function pickDaemonParticipant(
  room: Pick<Room, "remoteParticipants">,
): RemoteParticipant | null {
  for (const p of room.remoteParticipants.values()) {
    if (p.identity.startsWith("daemon-")) {
      return p;
    }
  }
  return null;
}

/** Override in tests (e.g. HTTP callback stub without WebRTC). */
export type StartOAuthTcpTunnelFn<R extends Pick<Room, "remoteParticipants" | "on">> = (
  opts: {
    room: R;
    targetIdentity: string;
    listenPort: number;
    remoteLoopbackPort: number;
    expectedState: string;
  },
) => { stop: () => void };

export type LiveKitOAuthRelayDeps<R extends Pick<Room, "remoteParticipants" | "on">> = {
  openUrlInBrowser: (url: string) => void;
  startOAuthTcpTunnel?: StartOAuthTcpTunnelFn<R>;
};

export type LiveKitOAuthRelayHandle = {
  dispose: () => void;
  /** Local TCP listen port when pending OAuth is active; otherwise `null`. */
  getCallbackPort: () => number | null;
};

function defaultStartOAuthTcpTunnel(
  opts: Parameters<StartOAuthTcpTunnelFn<Room>>[0],
): { stop: () => void } {
  return startOAuthLoopbackTcpProxy({
    room: opts.room,
    targetIdentity: opts.targetIdentity,
    listenPort: opts.listenPort,
    remoteLoopbackPort: opts.remoteLoopbackPort,
  });
}

/**
 * Watch daemon participant metadata, open authorize URL, listen on loopback TCP, tunnel bytes to the session host via LiveKit.
 */
export async function installLiveKitOAuthRelay<
  R extends Pick<Room, "remoteParticipants" | "on">,
>(room: R, deps: LiveKitOAuthRelayDeps<R>): Promise<LiveKitOAuthRelayHandle> {
  const { openUrlInBrowser: openUrl } = deps;
  const startTunnel = deps.startOAuthTcpTunnel ?? (defaultStartOAuthTcpTunnel as StartOAuthTcpTunnelFn<R>);

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

  room.on(RoomEvent.ParticipantConnected, () => {
    void onMetadata();
  });
  room.on(RoomEvent.ParticipantMetadataChanged, () => {
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

/**
 * Join LiveKit room, watch daemon metadata for `codex_oauth`, tunnel OAuth callback TCP via LiveKit.
 */
export async function runLiveKitOAuthRelay(options: {
  livekitUrl: string;
  roomName: string;
  rpcBaseUrl: string;
  identity: string;
}): Promise<void> {
  const { livekitUrl, roomName, rpcBaseUrl, identity } = options;

  const transport = createConnectTransport({
    baseUrl: rpcBaseUrl,
    useBinaryFormat: true,
  });
  const tokenClient = createClient(TokenService, transport);
  const res = await tokenClient.generateToken({ room: roomName, identity });
  const room = new Room();
  await room.connect(livekitUrl, res.token, {
    autoSubscribe: true,
    dynacast: true,
  });
  console.info(
    `[tddy-desktop] OAuth relay: LiveKit connected (room ${roomName}, identity ${identity})`,
  );

  await installLiveKitOAuthRelay(room, {
    openUrlInBrowser,
  });
}
