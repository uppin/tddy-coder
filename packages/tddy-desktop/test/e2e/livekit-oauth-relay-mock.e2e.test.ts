/**
 * Mock-room E2E: metadata → callback listener → injectable `deliverCallback` (no WebRTC).
 */
import { describe, expect, test } from "bun:test";
import type { Room } from "livekit-client";
import { RoomEvent } from "livekit-client";

import { installLiveKitOAuthRelay } from "../../src/bun/livekit-oauth-relay";

function codexMetadataJson(port: number, state: string) {
  return JSON.stringify({
    codex_oauth: {
      pending: true,
      authorize_url: `https://auth.openai.com/authorize?state=${encodeURIComponent(state)}`,
      callback_port: port,
      state,
    },
  });
}

function createMockRoom(daemon: { identity: string; metadata: string }) {
  const participants = new Map<string, typeof daemon>([["d", daemon]]);
  const handlers = new Map<string, Array<() => void>>();
  const room = {
    remoteParticipants: participants as unknown as Room["remoteParticipants"],
    on(ev: string, fn: () => void) {
      const list = handlers.get(ev) ?? [];
      list.push(fn);
      handlers.set(ev, list);
    },
    emit(ev: string) {
      for (const fn of handlers.get(ev) ?? []) {
        fn();
      }
    },
  };
  return room;
}

describe("livekit oauth relay (mock room E2E)", () => {
  test("pending metadata starts callback server; GET relays deliverCallback", async () => {
    let delivered: { code: string; state: string } | null = null;
    const opened: string[] = [];
    const daemon = { identity: "daemon-e2e", metadata: codexMetadataJson(0, "st-e2e") };
    const room = createMockRoom(daemon);

    const handle = await installLiveKitOAuthRelay(room, {
      openUrlInBrowser: (u) => {
        opened.push(u);
      },
      createDeliverPipeline: () => ({
        deliverCallback: async (code, state) => {
          delivered = { code, state };
        },
        dispose: () => {},
      }),
    });

    try {
      expect(opened.length).toBe(1);
      const port = handle.getCallbackPort();
      expect(port).not.toBeNull();
      const res = await fetch(
        `http://127.0.0.1:${port}/auth/callback?code=callback-code&state=st-e2e`,
      );
      expect(res.ok).toBe(true);
      expect(delivered).toEqual({ code: "callback-code", state: "st-e2e" });
    } finally {
      handle.dispose();
    }
  });

  test("state mismatch returns 403 and does not deliver", async () => {
    let delivered: { code: string; state: string } | null = null;
    const daemon = { identity: "daemon-e2e", metadata: codexMetadataJson(0, "want-state") };
    const room = createMockRoom(daemon);

    const handle = await installLiveKitOAuthRelay(room, {
      openUrlInBrowser: () => {},
      createDeliverPipeline: () => ({
        deliverCallback: async (code, state) => {
          delivered = { code, state };
        },
        dispose: () => {},
      }),
    });

    try {
      const port = handle.getCallbackPort();
      expect(port).not.toBeNull();
      const res = await fetch(
        `http://127.0.0.1:${port}/auth/callback?code=x&state=wrong`,
      );
      expect(res.status).toBe(403);
      expect(delivered).toBeNull();
    } finally {
      handle.dispose();
    }
  });

  test("ParticipantMetadataChanged starts server when daemon appears later", async () => {
    const daemon = { identity: "daemon-e2e", metadata: "" };
    const room = createMockRoom(daemon);

    const handle = await installLiveKitOAuthRelay(room, {
      openUrlInBrowser: () => {},
      createDeliverPipeline: () => ({
        deliverCallback: async () => {},
        dispose: () => {},
      }),
    });

    try {
      expect(handle.getCallbackPort()).toBeNull();
      daemon.metadata = codexMetadataJson(0, "late");
      room.emit(RoomEvent.ParticipantMetadataChanged);
      const deadline = Date.now() + 2_000;
      while (handle.getCallbackPort() === null && Date.now() < deadline) {
        await new Promise((r) => setTimeout(r, 10));
      }
      expect(handle.getCallbackPort()).not.toBeNull();
    } finally {
      handle.dispose();
    }
  });
});
