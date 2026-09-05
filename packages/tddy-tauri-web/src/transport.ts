/**
 * ConnectRPC transport over a webview IPC bridge — the browser half of the `tddy-tauri-rpc`
 * flavour.
 *
 * The host application exposes two commands: one registers a response channel for this page, one
 * carries a single request frame. That is a duplex frame pipe, so everything above the pipe (ids,
 * epochs, correlation, streaming) is `tddy-rpc-web`'s; this module is the adapter.
 */

import type { Transport } from "@connectrpc/connect";
import { Channel, invoke } from "@tauri-apps/api/core";
import {
  createEnvelopeTransport,
  mintClientEpoch,
  type FrameListener,
  type FramePipe,
} from "tddy-rpc-web";

/** The host application's IPC surface, as this transport needs it. */
export interface WebviewIpcBridge {
  /**
   * Register `onFrame` as this page's response channel, identified by `clientEpoch`. The host
   * abandons whatever the previous page opened.
   */
  connect(onFrame: (frame: Uint8Array) => void, clientEpoch: number): Promise<void>;
  /** Send one encoded `RpcRequest` frame to the host. */
  send(frame: Uint8Array): Promise<void>;
  /** Resolves with a reason when the bridge is permanently gone. */
  closed: Promise<string>;
}

export interface TauriTransportOptions {
  bridge: WebviewIpcBridge;
  /** Minted per page load when omitted. */
  clientEpoch?: number;
}

/**
 * The production bridge, over the host application's `invoke` and channel APIs.
 *
 * Both commands carry frames as bytes: the response channel is a `Channel<ArrayBuffer>` the host
 * writes `InvokeResponseBody::Raw` onto, and a request frame is the invoke body itself rather than
 * a field inside a JSON object. Nothing on this path is base64 or JSON.
 */
export function createTauriIpcBridge(): WebviewIpcBridge {
  return {
    async connect(onFrame: (frame: Uint8Array) => void, clientEpoch: number): Promise<void> {
      const channel = new Channel<ArrayBuffer>();
      channel.onmessage = (frame) => onFrame(new Uint8Array(frame));
      await invoke("tddy_rpc_connect", { channel, clientEpoch });
    },
    async send(frame: Uint8Array): Promise<void> {
      await invoke("tddy_rpc_send", frame);
    },
    // The host process and this page have one lifetime: there is no state in which the bridge is
    // gone and the page is still running to hear about it. A refused `connect` or `send` is how a
    // departed host is noticed, and `webviewFramePipe` already reports those.
    closed: new Promise<string>(() => {}),
  };
}

/**
 * `bridge` as a {@link FramePipe}.
 *
 * The response channel is registered as the pipe is built, and every frame waits for that
 * registration to complete: a frame the host answers into a channel it does not have yet would
 * leave its call unsettled forever.
 */
function webviewFramePipe(bridge: WebviewIpcBridge, clientEpoch: number): FramePipe {
  let listening: FrameListener | null = null;
  let gone = false;

  /**
   * Report the bridge permanently gone, once. A frame the host refused to accept has no answer
   * coming, and the page cannot tell which of the calls in flight the host still holds — so a
   * failure to send ends the connection rather than leaving one call silently unsettled.
   */
  const reportGone = (reason: string) => {
    if (gone) return;
    gone = true;
    listening?.onClose(reason);
  };

  const registered = bridge.connect((frame) => listening?.onFrame(frame), clientEpoch);
  void registered.catch((error: unknown) => {
    reportGone(`could not register a response channel with the host application: ${String(error)}`);
  });
  void bridge.closed.then((reason) => reportGone(reason));

  return {
    send(frame: Uint8Array) {
      void registered
        .then(() => bridge.send(frame))
        .catch((error: unknown) => {
          reportGone(`could not send a request frame to the host application: ${String(error)}`);
        });
    },
    subscribe(listener: FrameListener) {
      listening = listener;
      return () => {
        listening = null;
      };
    },
  };
}

/** A ConnectRPC transport that reaches the in-process daemon over `options.bridge`. */
export function createTauriTransport(options: TauriTransportOptions): Transport {
  const clientEpoch = options.clientEpoch ?? mintClientEpoch();
  return createEnvelopeTransport({
    pipe: webviewFramePipe(options.bridge, clientEpoch),
    clientEpoch,
    label: "the host application",
  });
}
