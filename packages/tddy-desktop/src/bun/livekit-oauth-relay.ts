import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import {
  CodexOAuthService,
  createLiveKitTransport,
  DeliverCallbackRequestSchema,
} from "tddy-livekit-web";
import {
  Room,
  RoomEvent,
  type RemoteParticipant,
} from "livekit-client";

import { parseCodexOAuthMetadata } from "./codex-oauth-metadata";
import { startOAuthCallbackServer } from "./oauth-callback-server";
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
      return p as RemoteParticipant;
    }
  }
  return null;
}

/** LiveKit data-channel path used after the browser hits the local callback URL. */
export type CodexOAuthDeliverPipeline = {
  deliverCallback: (code: string, state: string) => Promise<void>;
  dispose: () => void;
};

export function defaultCodexOAuthDeliverPipeline(
  room: Room,
  targetIdentity: string,
): CodexOAuthDeliverPipeline {
  const lkTransport = createLiveKitTransport({
    room,
    targetIdentity,
  });
  const oauthClient = createClient(CodexOAuthService, lkTransport);
  return {
    deliverCallback: async (code: string, state: string) => {
      await oauthClient.deliverCallback(
        create(DeliverCallbackRequestSchema, {
          code,
          state,
          sessionId: "",
        }),
      );
    },
    dispose: () => {
      lkTransport.destroy();
    },
  };
}

export type LiveKitOAuthRelayDeps<R extends Pick<Room, "remoteParticipants" | "on">> = {
  openUrlInBrowser: (url: string) => void;
  createDeliverPipeline: (
    room: R,
    targetIdentity: string,
  ) => CodexOAuthDeliverPipeline;
};

export type LiveKitOAuthRelayHandle = {
  dispose: () => void;
  /** OS port for the local callback server when pending OAuth is active; otherwise `null`. */
  getCallbackPort: () => number | null;
};

/**
 * Watch daemon participant metadata, open authorize URL, run callback server, relay to coder via `deliverCallback`.
 * Used by production (`runLiveKitOAuthRelay`) and by E2E tests with a mock `room`.
 */
export async function installLiveKitOAuthRelay<
  R extends Pick<Room, "remoteParticipants" | "on">,
>(room: R, deps: LiveKitOAuthRelayDeps<R>): Promise<LiveKitOAuthRelayHandle> {
  const { openUrlInBrowser: openUrl, createDeliverPipeline } = deps;

  let callbackServer: ReturnType<typeof startOAuthCallbackServer> | null = null;
  let lastAuthorize: string | null = null;

  const stopCallbackServer = () => {
    if (callbackServer) {
      callbackServer.stop();
      callbackServer = null;
    }
  };

  const onMetadata = async () => {
    const daemon = pickDaemonParticipant(room);
    if (!daemon) return;
    const info = parseCodexOAuthMetadata(daemon.metadata ?? "");
    if (!info?.pending || !info.authorizeUrl) {
      stopCallbackServer();
      lastAuthorize = null;
      return;
    }
    if (info.authorizeUrl !== lastAuthorize) {
      lastAuthorize = info.authorizeUrl;
      openUrl(info.authorizeUrl);
    }
    const port = info.callbackPort ?? 1455;
    const expectedState = info.state ?? "";
    if (callbackServer) {
      return;
    }
    const targetIdentity = daemon.identity;
    callbackServer = startOAuthCallbackServer({
      port,
      expectedState,
      onHit: async (hit) => {
        const pipeline = createDeliverPipeline(room, targetIdentity);
        try {
          await pipeline.deliverCallback(hit.code, hit.state);
        } finally {
          pipeline.dispose();
        }
        stopCallbackServer();
      },
    });
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
      stopCallbackServer();
    },
    getCallbackPort: () => callbackServer?.port ?? null,
  };
}

/**
 * Join LiveKit common room, watch daemon metadata for `codex_oauth`, run callback server, relay via RPC.
 * Requires `TDDY_RPC_BASE` (e.g. http://127.0.0.1:8899/rpc), `TDDY_LIVEKIT_URL`, `TDDY_LIVEKIT_ROOM`.
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
  await room.connect(livekitUrl, res.token);

  await installLiveKitOAuthRelay(room, {
    openUrlInBrowser,
    createDeliverPipeline: defaultCodexOAuthDeliverPipeline,
  });
}
