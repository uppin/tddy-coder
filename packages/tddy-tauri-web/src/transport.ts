/**
 * ConnectRPC transport over a webview IPC bridge — the browser half of the `tddy-tauri-rpc`
 * flavour.
 *
 * The host application exposes three commands: one opens a connection and registers a response
 * channel for it, one carries a single request frame, one releases a connection. That is a duplex
 * frame pipe with an explicit lifetime, so everything above the pipe (ids, epochs, correlation,
 * streaming) is `tddy-rpc-web`'s; this module is the adapter.
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
   * This connection's identity, minted with the bridge and fixed for its lifetime.
   *
   * The epoch a connection registers under and the epoch its request frames are stamped with must
   * be the same number, or the host answers into a channel nobody is listening on. Making it the
   * bridge's own — rather than something a transport built over the bridge decides — is what keeps
   * the two in step: a page that builds two transports over one bridge still holds one connection
   * under one epoch, which is exactly what {@link WebviewIpcHost} hands it.
   */
  readonly clientEpoch: number;

  /**
   * Release this connection: the host drops the peer's state, aborts the forwards still publishing
   * for it, and closes the sink.
   *
   * Leaving `closed` to never resolve was right while host and page shared one lifetime — true of
   * the daemon connection, which lasts exactly as long as the page does. It is **not** true of a
   * session connection, which ends when the session is detached. Without an explicit release every
   * attach leaks a host-side peer.
   *
   * Idempotent.
   */
  close(): Promise<void>;

  /**
   * Open this connection, with `onFrame` as its response channel and {@link clientEpoch} as its
   * identity.
   *
   * The connection stands alongside every other one this page holds: opening it disturbs none of
   * them, and only {@link close} — or the page going away — ends it. The host refuses an epoch an
   * open connection is already using rather than displacing the incumbent, whose calls in flight
   * would otherwise be lost to someone else's mistake — so a bridge is connected once, by the one
   * transport built over it, and a second `connect` on the same bridge is refused rather than
   * quietly taking the first one's channel away.
   */
  connect(onFrame: (frame: Uint8Array) => void): Promise<void>;
  /** Send one encoded `RpcRequest` frame to the host. */
  send(frame: Uint8Array): Promise<void>;
  /** Resolves with a reason when the bridge is permanently gone. */
  closed: Promise<string>;
}

export interface TauriTransportOptions {
  /**
   * The connection this transport speaks over, and — through {@link WebviewIpcBridge.clientEpoch} —
   * the identity its frames are stamped with. There is deliberately no way to name a different
   * epoch here: a transport that stamped frames with anything other than the epoch its bridge
   * registered under would have them refused by the host, and its calls would never settle.
   */
  bridge: WebviewIpcBridge;
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
 * `bridge` as a {@link FramePipe}.
 *
 * The response channel is registered as the pipe is built, and every frame waits for that
 * registration to complete: a frame the host answers into a channel it does not have yet would
 * leave its call unsettled forever.
 */
function webviewFramePipe(bridge: WebviewIpcBridge): FramePipe {
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

  const registered = bridge.connect((frame) => listening?.onFrame(frame));
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

/**
 * A ConnectRPC transport that reaches whatever `options.bridge` connects to — the in-process daemon
 * or one session — over the host application's IPC.
 *
 * The bridge names the connection's epoch; this transport only carries it, so the frames it stamps
 * and the registration they are answered through can never name different connections.
 */
export function createTauriTransport(options: TauriTransportOptions): Transport {
  return createEnvelopeTransport({
    pipe: webviewFramePipe(options.bridge),
    clientEpoch: options.bridge.clientEpoch,
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
 * A page used to hold exactly one, because the host served exactly one: registering a response
 * channel *abandoned the previous one*, so a page that opened two would have abandoned its own
 * first connection and left every call already issued on it waiting forever. With addressed
 * connections the invariant becomes **one bridge per target** rather than one per page, and this
 * registry is what holds it — which is why there is no exported way to build a bridge outside it.
 * One built outside would open a second connection to a target the page already reaches, under a
 * second epoch that nothing would ever release.
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
  // This connection's identity, minted here because this is what a connection *is* on the page
  // side: one bridge, one epoch, one host-side peer. Minted by whoever built a transport instead,
  // it would be minted once per transport, and a page that builds two over the one bridge would
  // open two connections to a target it reaches once — the second stamping frames with an epoch
  // the first registration never named.
  const clientEpoch = mintClientEpoch();
  /**
   * What this bridge asked the host to register, once `connect` has asked. A bridge that never
   * asked has no host-side peer, so there is nothing for `close` to release — and the pending
   * registration is kept, not just the fact of it, because a `close` racing a mount would otherwise
   * leave behind the very peer the registration in flight is still creating.
   */
  let registration: Promise<unknown> | null = null;
  /**
   * Whether the page has given this bridge up. It gates `connect` as well as `close`, because the
   * two orderings leak in opposite directions and only one of them is covered by keeping the
   * pending registration above: a `connect` that lands *after* `close` would register a peer that
   * `close` — idempotent, and already run — will never come back to release.
   */
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
    clientEpoch,
    async connect(onFrame: (frame: Uint8Array) => void): Promise<void> {
      // A released bridge registers nothing. The registry has already dropped this entry, so the
      // peer such a registration created would be reachable from nowhere — exactly the leak `close`
      // exists to prevent. Returning quietly rather than throwing is enough on its own: `closed`
      // resolved as the bridge was released, so the transport built over it has already been told
      // this connection is gone and will settle its calls without waiting on a channel.
      if (released) return;
      const channel = new Channel<ArrayBuffer>();
      channel.onmessage = (frame) => onFrame(new Uint8Array(frame));
      const registered = invoke("tddy_rpc_connect", { channel, clientEpoch, target });
      registration = registered;
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
      const registered = await held.then(
        () => true,
        () => false,
      );
      if (!registered) return;
      await invoke("tddy_rpc_disconnect", { clientEpoch });
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
 * rather than opening two connections to it — two peers, two epochs, twice the host-side state, and
 * a release from either leaving the other still open.
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
