/**
 * The desktop build's own host: reached over IPC, contributed to the directory, and registered
 * ahead of LiveKit.
 *
 * Everything else in this stack was preparation for this file. Nodes 1–4 gave `tddy-web` a provider
 * registry, a source-merged host directory, one session connection carrying capabilities, and
 * surfaces gated on them; node 6 gave the host application concurrent addressed IPC connections.
 * What was still missing is the one thing `tddy-web` must never contain: knowledge of a particular
 * wire. It arrives here, from the desktop build, through the same registries any provider uses.
 *
 * **The browser is protected behaviourally, not structurally.** `packages/tddy-desktop` is a Tauri
 * shell over `packages/tddy-web/dist`: one bundle, one entry (`src/index.tsx`), served to browsers
 * by the daemon and loaded by the shell alike. So this module *is* in the browser's bundle — there
 * was never a build in which it could not be — and what keeps a browser off the IPC path is that
 * {@link localHostRegistrationFor} answers `null` for it, so nothing is ever registered. That is the
 * one runtime question `../daemonTransportFlavour` already answers to choose this page's own daemon
 * transport, asked once more rather than asked a second way; there is no separate notion of "is this
 * the desktop" anywhere. `DesktopIpcHostAcceptance.cy.tsx` pins it from both sides.
 *
 * PRD: `docs/dev/1-WIP/2026-09-05-optional-livekit-desktop-ipc-host-prd.md`.
 */

import { createClient, type Client, type Transport } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import {
  DAEMON_TARGET,
  sessionTarget,
  thisPagesIpcHost,
  type ConnectionTarget,
  type WebviewIpcBridge,
  type WebviewIpcHost,
} from "tddy-tauri-web";
import { ConnectionService } from "../../gen/connection_pb";
import { SELF_LABEL_SUFFIX } from "../../lib/participantRole";
import { daemonTransportFlavour, type TauriHostWindow } from "../daemonTransportFlavour";
import { hostDescriptorOf } from "../hostDirectory/daemonHost";
import type { HostDirectorySource } from "../hostDirectory/types";
import type { SessionAttachmentHint, SessionConnection } from "./session";
import type { TerminalFeed, TerminalOptions } from "./terminal";
import { openDaemonTerminalFeed, TerminalResumePoint } from "./terminalFeed";
import type {
  ConnectionCapability,
  ConnectionProvider,
  ConnectionStatus,
  HostConnection,
} from "./types";

/** The id this provider registers under. Precedence is stated against it, so it is a constant. */
export const IPC_PROVIDER_ID = "ipc";

/** The id this directory source contributes under, for the same reason. */
export const LOCAL_IPC_SOURCE_ID = "local-ipc";

/**
 * A frame pipe carries calls and nothing else — no tracks, no participant roster. Shared by every
 * connection this module issues, because the set is a property of the wire and not of the machine
 * at the other end of it; no caller may mutate it.
 */
const IPC_CAPABILITIES: ReadonlySet<ConnectionCapability> = new Set<ConnectionCapability>(["rpc"]);

/** What the desktop build knows about itself when it registers. */
export interface LocalHostRegistration {
  /**
   * The daemon instance serving this page, from `DaemonConfigService.GetClientConfig`'s
   * `daemonInstanceId` — the same payload a browser reads from `/api/config`.
   *
   * Available **before sign-in**, because the daemon serves that call ungated. That matters: the
   * LiveKit path cannot produce a host until authentication completes, since it needs a presence
   * identity derived from the user's login. The IPC path has no such gate, so the desktop app has a
   * usable host from its first paint.
   */
  readonly daemonInstanceId: string;

  /** How the host is named in the selector. */
  readonly label: string;
}

/**
 * The local host `win` has, or `null` when it has none.
 *
 * The whole of "is the IPC path available here", asked once. It is not a new question: reaching the
 * daemon that served this page already depends on it, and `daemonTransportFlavour` is where that is
 * decided — a browser page posts to `{origin}/rpc`, a page the host application loaded has no origin
 * and goes over the IPC bridge. A host reached over IPC is available on exactly the same terms, so
 * asking a second, differently-worded question ("is this the desktop") would be inventing a way for
 * the two answers to disagree.
 *
 * `null` for a browser, and `null` when the daemon named no instance — a bundle served by something
 * that is not a daemon has no local host, which is the state a Storybook build is in. A `null`
 * registration is what {@link createIpcConnectionProvider} is never called with, and so what keeps
 * the browser bundle from registering a wire it cannot use.
 *
 * The label matches the one a daemon publishes for itself into a common room
 * (`SELF_LABEL_SUFFIX`), so the selector reads the same in both hosts and a machine described by
 * both sources does not change name depending on which account of it the directory kept.
 */
export function localHostRegistrationFor(
  win: TauriHostWindow,
  daemonInstanceId: string | undefined,
): LocalHostRegistration | null {
  if (daemonTransportFlavour(win) !== "webview-ipc") return null;
  const hostId = daemonInstanceId?.trim() ?? "";
  if (!hostId) return null;
  return { daemonInstanceId: hostId, label: `${hostId}${SELF_LABEL_SUFFIX}` };
}

/**
 * How this page reaches the host application — the injection seam, mirroring
 * `DaemonHostEnvironment` in `../daemonTransport`.
 *
 * Both fields are the registration site's to supply, for the same reason `LiveKitConnections` hands
 * `LiveKitConnectionProvider` its transport factory: they come from React context (the traffic meter
 * registry and the auth-token gate) and cannot be reached from a plain factory function.
 */
export interface LocalHostWiring {
  /**
   * This page's connections to the host application. Defaults to the page's own — there is exactly
   * one per page, and {@link thisPagesIpcHost} is the thing that holds it.
   */
  readonly ipc?: WebviewIpcHost;

  /**
   * Wrap one bridge as a transport carrying this page's interceptor stack — traffic metering, and
   * the auth gate that puts a request-time-fresh access token on every call.
   * `createDefaultWebviewIpcTransport` in `../daemonTransport` is that function, and the meter and
   * gate it needs are the registration site's.
   *
   * There is deliberately **no default**. A transport built without the gate would send whatever
   * token each request happened to carry, and a webview that has been open longer than an access
   * token lives carries a stale one — so a provider registered without this refuses to build a
   * transport rather than quietly sending credentials that will not do. The refusal names what is
   * missing, on the same terms as a LiveKit session opened without a token client.
   */
  readonly transportFor?: (bridge: WebviewIpcBridge) => Transport;
}

/**
 * One addressed IPC connection: the bridge to a target, the transport over it, and the clients built
 * on that transport.
 *
 * The bridge is opened on first use rather than up front, so resolving a host — which the registry
 * does for every screen that names one, whether or not it goes on to call anything — costs the host
 * application nothing. Clients are memoised per service for the life of the channel, which is the
 * identity guarantee `HostConnection.clientFor` and `SessionConnection.clientFor` both make.
 */
class IpcChannel {
  private bridge: WebviewIpcBridge | null = null;
  private builtTransport: Transport | null = null;
  private readonly clients = new Map<DescService, Client<DescService>>();

  /** Why the host application will not carry this connection any more; `null` while it will. */
  private failure: string | null = null;

  private released = false;

  constructor(
    private readonly target: ConnectionTarget,
    private readonly ipc: WebviewIpcHost,
    private readonly transportFor: ((bridge: WebviewIpcBridge) => Transport) | undefined,
    /** How this connection is named in a refusal. */
    private readonly describe: () => string,
  ) {}

  /**
   * `connected` unless the host application has said otherwise.
   *
   * There is no state in which the daemon is unreachable and this page is still running to notice:
   * the host application loaded the page and the daemon runs in its process. So the "not asked yet"
   * reading of `idle` never applies to a live channel — a screen that has resolved this host can
   * issue a call at any moment — and `error` appears only once the host application has reported the
   * connection permanently gone, which is the first real failure mode any provider in this app has.
   *
   * A *released* channel is `idle`: nothing is being asked of it any more, which is a different
   * claim from the host having failed. Same rule `openHostServedSession` follows on close.
   */
  get status(): ConnectionStatus {
    if (this.released) return "idle";
    return this.failure === null ? "connected" : "error";
  }

  get error(): string | null {
    return this.released ? null : this.failure;
  }

  transport(): Transport {
    this.refuseIfClosed();
    this.builtTransport ??= this.open();
    return this.builtTransport;
  }

  clientFor<S extends DescService>(service: S): Client<S> {
    this.refuseIfClosed();
    const cached = this.clients.get(service);
    if (cached) return cached as Client<S>;
    const built = createClient(service, this.transport());
    this.clients.set(service, built as Client<DescService>);
    return built;
  }

  /**
   * Release the host-side peer this channel opened, if it opened one.
   *
   * Idempotent, and the whole reason a session connection is closeable over this wire: the host
   * holds per-connection state keyed by client epoch, so a detach that merely forgot its connection
   * would leak a peer and the forwards still publishing for it.
   */
  release(): void {
    if (this.released) return;
    this.released = true;
    const held = this.bridge;
    this.bridge = null;
    void held?.close();
  }

  /**
   * Refuse to do anything on a released channel.
   *
   * A call issued after a detach has no answer coming — the host-side peer is gone — and saying so
   * beats leaving it unsettled. Public because opening a terminal is not routed through
   * {@link clientFor} on this channel and has to make the same check.
   */
  refuseIfClosed(): void {
    if (this.released) throw new Error(`${this.describe()} is closed`);
  }

  private open(): Transport {
    if (!this.transportFor) {
      throw new Error(
        `${this.describe()} cannot be reached: this provider was registered without the ` +
          `transport factory that carries the page's auth gate and traffic meter`,
      );
    }
    const bridge = this.ipc.openConnection(this.target);
    this.bridge = bridge;
    void bridge.closed.then((reason) => {
      this.failure = reason;
    });
    return this.transportFor(bridge);
  }
}

/**
 * The desktop's own host, reached on the `Daemon`-targeted IPC connection.
 *
 * That is the same connection `daemonTransport.ts` already reaches this page's daemon over — the
 * host application holds one bridge per target — so a page that has both does not open two peers for
 * the one thing.
 */
class IpcHostConnection implements HostConnection {
  readonly providerId = IPC_PROVIDER_ID;
  readonly capabilities = IPC_CAPABILITIES;

  private readonly channel: IpcChannel;

  constructor(
    readonly hostId: string,
    private readonly ipc: WebviewIpcHost,
    private readonly transportFor: ((bridge: WebviewIpcBridge) => Transport) | undefined,
  ) {
    this.channel = new IpcChannel(
      DAEMON_TARGET,
      ipc,
      transportFor,
      () => `host ${hostId} over the host application's IPC bridge`,
    );
  }

  get status(): ConnectionStatus {
    return this.channel.status;
  }

  get error(): string | null {
    return this.channel.error;
  }

  transport(): Transport {
    return this.channel.transport();
  }

  clientFor<S extends DescService>(service: S): Client<S> {
    return this.channel.clientFor(service);
  }

  /**
   * A connection to one session on this host, on its **own** `Session`-targeted IPC connection.
   *
   * The hint's LiveKit fields are not read, and not because they are unsupported: a session on this
   * host is served by this host, and routing it out to a media server and back would make the
   * desktop's own sessions require the thing this stack made optional. So a session reached this way
   * advertises `{"rpc"}` whatever the daemon advertised about it, and no room, participant, token or
   * LiveKit identity exists anywhere on this path.
   *
   * Not memoised: two attachments of the same session are two attachments, each with its own
   * `close()`. Note the host application's registry does key its *bridges* by target, so two live
   * attachments of the same session share one — the first `close()` releases it. Which connection
   * owns a shared bridge is node 6's to say; nothing here may second-guess it.
   */
  openSession(sessionId: string, hint: SessionAttachmentHint): SessionConnection {
    if (hint.sessionId !== sessionId) {
      throw new Error(`openSession(${sessionId}) was given a hint for session ${hint.sessionId}`);
    }
    return openIpcSession(this, this.ipc, this.transportFor, sessionId);
  }
}

/**
 * `sessionId` on `host`, over a `Session`-targeted IPC connection of its own.
 *
 * Separate and concurrent, which is exactly what node 6 made possible: several attached sessions
 * hold several connections, and detaching one releases only that one.
 */
function openIpcSession(
  host: IpcHostConnection,
  ipc: WebviewIpcHost,
  transportFor: ((bridge: WebviewIpcBridge) => Transport) | undefined,
  sessionId: string,
): SessionConnection {
  const channel = new IpcChannel(
    sessionTarget(sessionId),
    ipc,
    transportFor,
    () => `session ${sessionId} on host ${host.hostId}`,
  );

  // One resume point per terminal, for the life of this connection: a session has several terminals
  // and each is at its own offset, so re-opening one must not resume it from another's.
  const resumePoints = new Map<string, TerminalResumePoint>();
  const resumePointFor = (terminalId: string): TerminalResumePoint => {
    const existing = resumePoints.get(terminalId);
    if (existing) return existing;
    const fresh = new TerminalResumePoint();
    resumePoints.set(terminalId, fresh);
    return fresh;
  };

  return {
    hostId: host.hostId,
    sessionId,
    get status(): ConnectionStatus {
      return channel.status;
    },
    get error(): string | null {
      return channel.error;
    },
    capabilities: IPC_CAPABILITIES,
    clientFor<S extends DescService>(service: S): Client<S> {
      return channel.clientFor(service);
    },
    transport(): Transport {
      return channel.transport();
    },
    close(): void {
      channel.release();
    },
    /**
     * The terminal over the *host's* `ConnectionService`, not this session's connection.
     *
     * The daemon holds the capture ring, so scrollback and the offset-anchored resume come from it —
     * the same reason `openHostServedSession` and the LiveKit session connection both ask the host
     * for history rather than the session.
     */
    openTerminal(options: TerminalOptions): TerminalFeed {
      channel.refuseIfClosed();
      return openDaemonTerminalFeed({
        client: host.clientFor(ConnectionService),
        sessionId,
        resume: resumePointFor(options.terminalId ?? ""),
        options,
      });
    },
  };
}

/**
 * The connection provider for the desktop's own host.
 *
 * Registered **first**, so it wins for the host it claims even when a common room is configured and
 * could also reach that machine. The daemon runs in the same process as the webview host, so
 * reaching it through a media server is a round trip out of the machine and back to a roster already
 * in the binary. Precedence expresses that without a user-facing preference.
 *
 * Its connections advertise `{"rpc"}` and nothing else. The daemon *could* publish media into a
 * LiveKit room, but that would make the desktop's own host quietly require the thing this stack made
 * optional — so the media surfaces are absent, which node 4 already handles.
 *
 * It claims **exactly one** host, and returning `null` for every other is what leaves the peers to
 * the LiveKit provider registered behind it.
 */
export function createIpcConnectionProvider(
  registration: LocalHostRegistration,
  wiring: LocalHostWiring = {},
): ConnectionProvider {
  const hostId = registration.daemonInstanceId.trim();
  const ipc = wiring.ipc ?? thisPagesIpcHost();
  // One connection for the host, so `clientFor` is stable for as long as this provider is
  // registered — the same guarantee `LiveKitConnectionProvider` makes, for the same reason.
  let connection: HostConnection | null = null;

  return {
    id: IPC_PROVIDER_ID,
    connectHost(asked: string): HostConnection | null {
      // A registration naming no instance claims nothing: a provider that answered to `""` would
      // shadow every other wire for a host id nobody has.
      if (!hostId || asked !== hostId) return null;
      connection ??= new IpcHostConnection(hostId, ipc, wiring.transportFor);
      return connection;
    },
  };
}

/**
 * The desktop's own host, as a host-directory source.
 *
 * It contributes exactly one entry — the daemon serving this page, from `daemonInstanceId` — and
 * reports `connected`, because there is nothing to connect to: the daemon is in this process and the
 * page was loaded by the application hosting it. That is what gives the desktop app a selectable
 * host from its first paint, before sign-in and with no common room anywhere.
 *
 * Registered **ahead of** the LiveKit source, so that where a common room also advertises this
 * machine the directory's first-source-wins merge keeps this description of it. Its own `sourceId`
 * is how that preference is expressed and how a diagnostic can say which account of the host won.
 *
 * `idle` with no hosts when the daemon named no instance — a page served by something that is not a
 * daemon has no local host to offer, exactly as `useServingHostDirectorySource` reports.
 */
export function createLocalHostDirectorySource(
  registration: LocalHostRegistration,
): HostDirectorySource {
  const hostId = registration.daemonInstanceId.trim();
  if (!hostId) return { id: LOCAL_IPC_SOURCE_ID, status: "idle", error: null, hosts: [] };
  const label = registration.label.trim() || `${hostId}${SELF_LABEL_SUFFIX}`;
  return {
    id: LOCAL_IPC_SOURCE_ID,
    status: "connected",
    error: null,
    hosts: [hostDescriptorOf({ instanceId: hostId, label }, LOCAL_IPC_SOURCE_ID)],
  };
}

/**
 * Whether LiveKit should be brought up at all, from the configuration the daemon served.
 *
 * The exact definition of "if settings are configured": both a URL and a common room, non-blank.
 * With either missing the LiveKit source contributes nothing, constructs no `Room`, and calls no
 * `TokenService` — and reports `idle`, not `error`, because an operator who deliberately did not
 * configure LiveKit must not be shown a connection failure for it on every screen.
 *
 * That behaviour is already enforced, by `hooks/useCommonRoom`'s own guard, which short-circuits
 * before it mints a token or constructs a `Room`. This is the rule stated by name, for a caller that
 * has to decide *before* rendering the hook — which is where a registration site asks it. The two
 * are not consolidated because `useCommonRoom` belongs to a parent node of this stack and rewriting
 * its guard from here would be reaching into somebody else's surface for a tidiness that changes no
 * behaviour.
 */
export function liveKitIsConfigured(config: {
  livekitUrl?: string;
  commonRoom?: string;
}): boolean {
  return Boolean(config.livekitUrl?.trim()) && Boolean(config.commonRoom?.trim());
}
