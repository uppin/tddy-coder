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
   * Release this connection: the host drops the peer's state, aborts the forwards still publishing
   * for it, and closes the sink.
   *
   * `closed` never resolving was right while host and page shared one lifetime — true of the
   * daemon connection, and the stated reason `createTauriIpcBridge` leaves it pending forever. It
   * is **not** true of a session connection, which ends when the session is detached. Without an
   * explicit release every attach leaks a host-side peer.
   *
   * Idempotent.
   */
  close(): Promise<void>;

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
  /**
   * Receives a line per call event. Omitted, calls are not logged.
   *
   * The host application is the one place a stalled call has no other visible symptom — there is no
   * network panel to read it off, and a call that never settles renders as a screen that never
   * arrives. The caller supplies the sink so this package needs no logging dependency of its own.
   */
  log?: (message: string) => void;
}

/**
 * The production bridge to the daemon serving this page, over the host application's `invoke` and
 * channel APIs.
 *
 * A bridge built here is nobody's but the caller's: releasing it releases the host-side peer and
 * nothing else. {@link thisPagesIpcHost} is what holds one bridge per target for the whole page.
 */
export function createTauriIpcBridge(): WebviewIpcBridge {
  return createTauriIpcBridgeTo(DAEMON_TARGET, () => {});
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
    log: options.log,
  });
}

// ---------------------------------------------------------------------------
// Many connections per page
// ---------------------------------------------------------------------------

/**
 * What a connection asks to reach, mirroring `tddy-tauri-rpc`'s `ConnectionTarget`.
 *
 * A closed union rather than a string, for the same reason it is a closed enum on the host side: an
 * open target would let the LiveKit identity strings this stack removes leak across the IPC
 * boundary, and nothing would notice.
 */
export type ConnectionTarget =
  | { readonly kind: "daemon" }
  | { readonly kind: "session"; readonly sessionId: string };

/** The daemon serving this page. */
export const DAEMON_TARGET: ConnectionTarget = { kind: "daemon" };

/** One session's own RPC. */
export function sessionTarget(sessionId: string): ConnectionTarget {
  return { kind: "session", sessionId };
}

/**
 * The page's connections to the host application.
 *
 * `createTauriIpcBridge` opens exactly one, because the host used to serve exactly one: registering
 * a response channel *abandoned the previous one*, so a page that opened two would have abandoned
 * its own first connection and left every call already issued on it waiting forever. With addressed
 * connections that is no longer true, and the invariant becomes **one bridge per target** rather
 * than one per page.
 */
export interface WebviewIpcHost {
  /**
   * The bridge for `target`, opening one if this page has none.
   *
   * Called twice for the same target it returns the same bridge — the daemon connection is still
   * opened exactly once per page, which is what `daemonTransport.ts`'s module-level singleton was
   * protecting.
   */
  openConnection(target: ConnectionTarget): WebviewIpcBridge;
}

/**
 * A bridge to `target`, over the host application's `invoke` and channel APIs.
 *
 * Both commands carry frames as bytes: the response channel is a `Channel<ArrayBuffer>` the host
 * writes `InvokeResponseBody::Raw` onto, and a request frame is the invoke body itself rather than
 * a field inside a JSON object. Nothing on this path is base64 or JSON. The target rides along with
 * the registration only: a frame is routed by the `clientEpoch` it is stamped with, which is the
 * epoch this bridge registered under, so the send path needs no target and must not grow one.
 *
 * Neither command is invoked while the bridge is merely being built — `openConnection` may run
 * for a component that never issues a call, and the host is asked for a peer only once one is
 * wanted.
 *
 * `onReleased` runs when `close` releases the connection, before the host is told: the page's
 * registry drops its entry there, so a target reattached while the release is still in flight opens
 * a fresh connection rather than being handed one whose peer is going away.
 */
function createTauriIpcBridgeTo(
  target: ConnectionTarget,
  onReleased: () => void,
): WebviewIpcBridge {
  /**
   * What this bridge asked the host to register, once `connect` has asked. A bridge that never
   * asked has no host-side peer, so there is nothing for `close` to release — and the pending
   * registration is kept, not just the epoch, because a `close` racing a mount would otherwise
   * leave behind the very peer the registration in flight is still creating.
   */
  let registration: { readonly clientEpoch: number; readonly registered: Promise<unknown> } | null =
    null;
  let released = false;

  let reportGone: (reason: string) => void = () => {};
  // The host process and this page have one lifetime: there is no state in which the host is gone
  // and the page is still running to hear about it. A refused `connect` or `send` is how a departed
  // host is noticed, and `webviewFramePipe` already reports those. What does end this connection on
  // its own is the page releasing it, which is what resolves this.
  const closed = new Promise<string>((resolve) => {
    reportGone = resolve;
  });

  return {
    async connect(onFrame: (frame: Uint8Array) => void, clientEpoch: number): Promise<void> {
      const channel = new Channel<ArrayBuffer>();
      channel.onmessage = (frame) => onFrame(new Uint8Array(frame));
      const registered = invoke("tddy_rpc_connect", { channel, clientEpoch, target });
      registration = { clientEpoch, registered };
      await registered;
    },
    async send(frame: Uint8Array): Promise<void> {
      await invoke("tddy_rpc_send", frame);
    },
    closed,
    async close(): Promise<void> {
      if (released) return;
      released = true;
      const held = registration;
      registration = null;
      onReleased();
      reportGone("the page released this connection");
      if (held === null) return;

      // A registration the host refused left no peer behind, so asking it to forget one would be
      // asking about something it never held. `connect`'s caller has already been told it failed.
      const registered = await held.registered.then(
        () => true,
        () => false,
      );
      if (!registered) return;
      await invoke("tddy_rpc_disconnect", { clientEpoch: held.clientEpoch });
    },
  };
}

/**
 * `target` as a key.
 *
 * The union has no identity of its own — `sessionTarget(id)` called twice yields two objects that
 * are equal in every way that matters and `===` in none — so the registry keys on what the target
 * *says* rather than on the object saying it.
 */
function connectionKey(target: ConnectionTarget): string {
  return target.kind === "daemon" ? "daemon" : `session:${target.sessionId}`;
}

/**
 * The connections this page holds, keyed by target.
 *
 * Module-level, because the page is what owns them: the daemon connection is opened once however
 * many call sites ask for it, and two call sites reaching the same session share the one bridge
 * rather than each registering a response channel and displacing the other.
 */
const openConnections = new Map<string, WebviewIpcBridge>();

const pagesIpcHost: WebviewIpcHost = {
  openConnection(target: ConnectionTarget): WebviewIpcBridge {
    const key = connectionKey(target);
    const alreadyOpen = openConnections.get(key);
    if (alreadyOpen) return alreadyOpen;

    const bridge: WebviewIpcBridge = createTauriIpcBridgeTo(target, () => {
      // Only if this bridge is still the one held: a target reattached while its predecessor was
      // being released has already put a fresh bridge under this key, and dropping that one would
      // leave the next caller opening a third connection to a session that has two.
      if (openConnections.get(key) === bridge) openConnections.delete(key);
    });
    openConnections.set(key, bridge);
    return bridge;
  },
};

/** This page's host-application connections. */
export function thisPagesIpcHost(): WebviewIpcHost {
  return pagesIpcHost;
}
