/**
 * The desktop build's own host: reached over IPC, and contributed to the directory.
 *
 * Its **connection provider** is registered ahead of the LiveKit one, which is what keeps the
 * machine the operator is sitting at off the media server. Its **directory source** is not — see
 * {@link createLocalHostDirectorySource}; the two orderings answer different questions and only the
 * first one is about reachability.
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
  sessionTarget,
  thisPagesIpcHost,
  type ConnectionTarget,
  type WebviewIpcBridge,
  type WebviewIpcHost,
} from "tddy-tauri-web";
import { ConnectionService } from "../../gen/connection_pb";
import { tddyDebug } from "../../lib/debugMask";
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

/** On the `tddy:rpc*` mask the rest of the IPC path already logs under. */
const dIpc = tddyDebug("tddy:rpc:local-host");

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
 * Every field is the registration site's to supply, for the same reason `LiveKitConnections` hands
 * `LiveKitConnectionProvider` its transport factory: they come from React context and cannot be
 * reached from a plain factory function.
 */
export interface LocalHostWiring {
  /**
   * This page's connections to the host application. Defaults to the page's own — there is exactly
   * one per page, and {@link thisPagesIpcHost} is the thing that holds it.
   */
  readonly ipc?: WebviewIpcHost;

  /**
   * The transport this page **already holds** to the daemon serving it — `useHttpTransport()`, which
   * inside the host application is the IPC transport over the `Daemon`-targeted bridge.
   *
   * Handed in rather than built here, and that is not a convenience. A bridge owns one connection
   * and one epoch (`tddy-tauri-web`'s `WebviewIpcBridge.clientEpoch`), `openConnection` returns the
   * *same* bridge for the daemon target every time it is asked, and building a transport over a
   * bridge connects it. So a second transport over the page's daemon bridge would re-register an
   * epoch the host already holds, `tddy_rpc_connect` would answer `EpochInUse`, and every call this
   * connection made would fail — while the rejected registration replaced the good one on the shared
   * bridge and disarmed its release. There is exactly one daemon connection per page; this is it.
   */
  readonly hostTransport?: Transport;

  /**
   * Wrap one **session** bridge as a transport carrying this page's interceptor stack — traffic
   * metering, and the auth gate that puts a request-time-fresh access token on every call.
   * `createDefaultWebviewIpcTransport` in `../daemonTransport` is that function, and the meter and
   * gate it needs are the registration site's.
   *
   * Sessions, and only sessions: `openConnection(sessionTarget(id))` genuinely mints a fresh bridge
   * under an epoch of its own, so there is a connection here to make and this is what makes it.
   *
   * There is deliberately **no default**. A transport built without the gate would send whatever
   * token each request happened to carry, and a webview that has been open longer than an access
   * token lives carries a stale one — so a provider registered without this refuses to build a
   * transport rather than quietly sending credentials that will not do. The refusal names what is
   * missing, on the same terms as a LiveKit session opened without a token client.
   */
  readonly transportFor?: (bridge: WebviewIpcBridge) => Transport;
}

/** The wiring with its defaults settled, so nothing below re-derives them per connection. */
interface ResolvedWiring {
  readonly ipc: WebviewIpcHost;
  readonly hostTransport: Transport | undefined;
  readonly transportFor: ((bridge: WebviewIpcBridge) => Transport) | undefined;
}

/** Clients memoised per service over one transport — the identity guarantee both connections make. */
class MemoisedClients {
  private readonly built = new Map<DescService, Client<DescService>>();

  clientFor<S extends DescService>(service: S, transport: () => Transport): Client<S> {
    const cached = this.built.get(service);
    if (cached) return cached as Client<S>;
    const client = createClient(service, transport());
    this.built.set(service, client as Client<DescService>);
    return client;
  }

  forget(): void {
    this.built.clear();
  }
}

/**
 * The one IPC connection this page holds to one session, and the attachments sharing it.
 *
 * Sharing is not a choice this module makes — the host application keys its bridges by target, so
 * two attachments of the same session *are* one connection whatever anyone above thinks. What is
 * chosen here is what that means for `close()`: attachments are counted, and the host-side peer is
 * released when the last one lets go. Releasing on the first would take the wire out from under a
 * screen still watching the session (`useSessionAttachment` is per-screen, so two screens on one
 * session is ordinary), and never releasing would leak a peer per session for the life of the page.
 *
 * The bridge is opened on first use rather than on attach: a session connection handed to a screen
 * that has not called anything yet costs the host application nothing.
 */
class IpcSessionWire {
  private bridge: WebviewIpcBridge | null = null;
  private builtTransport: Transport | null = null;
  private readonly clients = new MemoisedClients();

  /** Why the host application will not carry this connection any more; `null` while it will. */
  private failure: string | null = null;

  private attachments = 0;

  constructor(
    private readonly sessionId: string,
    private readonly wiring: ResolvedWiring,
    /** Drops this wire from the host connection, so the next attach opens a fresh one. */
    private readonly onLastDetached: () => void,
  ) {}

  /** `connected` unless the host application has reported this connection permanently gone. */
  get status(): ConnectionStatus {
    return this.failure === null ? "connected" : "error";
  }

  get error(): string | null {
    return this.failure;
  }

  attach(): void {
    this.attachments += 1;
  }

  detach(): void {
    this.attachments -= 1;
    if (this.attachments > 0) return;
    this.onLastDetached();
    const held = this.bridge;
    this.bridge = null;
    this.builtTransport = null;
    this.clients.forget();
    // A disconnect the host refuses is nothing this page can act on — the peer is one it has already
    // stopped using, and there is no second way to ask. It is logged rather than dropped so the
    // leak it implies is findable, and rather than left unhandled so it cannot surface as a rejected
    // promise in a screen that has nothing to do with it.
    if (held) {
      void held.close().catch((error: unknown) => {
        dIpc("releasing session %s failed: %o", this.sessionId, error);
      });
    }
  }

  transport(): Transport {
    this.builtTransport ??= this.open();
    return this.builtTransport;
  }

  clientFor<S extends DescService>(service: S): Client<S> {
    return this.clients.clientFor(service, () => this.transport());
  }

  private open(): Transport {
    const { ipc, transportFor } = this.wiring;
    if (!transportFor) {
      throw new Error(
        `session ${this.sessionId} cannot be reached: this provider was registered without the ` +
          `transport factory that carries the page's auth gate and traffic meter`,
      );
    }
    const bridge = ipc.openConnection(sessionTarget(this.sessionId));
    this.bridge = bridge;
    void bridge.closed.then((reason) => {
      this.failure = reason;
    });
    return transportFor(bridge);
  }
}

/**
 * The desktop's own host, over the daemon connection this page already holds.
 *
 * It builds no wire of its own. The page has exactly one connection to its daemon — opened by
 * `daemonTransport.ts` before any screen renders — and the local host *is* that daemon, so reaching
 * it is a matter of using the transport rather than making one. See {@link LocalHostWiring.hostTransport}
 * for what happens if it makes one instead.
 */
class IpcHostConnection implements HostConnection {
  readonly providerId = IPC_PROVIDER_ID;
  readonly capabilities = IPC_CAPABILITIES;

  /**
   * Structurally `null`, and {@link status} structurally `connected`.
   *
   * The host application loaded this page and the daemon runs in its process: there is no state in
   * which this connection is unusable and something is still rendering to say so. A *session* over
   * IPC does have a failure mode — its bridge can be refused or released — and reports it.
   */
  readonly error: string | null = null;
  readonly status: ConnectionStatus = "connected";

  private readonly clients = new MemoisedClients();

  /** One wire per session, because the host application holds one bridge per session. */
  private readonly sessions = new Map<string, IpcSessionWire>();

  constructor(
    readonly hostId: string,
    private readonly wiring: ResolvedWiring,
  ) {}

  transport(): Transport {
    if (!this.wiring.hostTransport) {
      throw new Error(
        `host ${this.hostId} cannot be reached: this provider was registered without the page's ` +
          `own daemon transport`,
      );
    }
    return this.wiring.hostTransport;
  }

  clientFor<S extends DescService>(service: S): Client<S> {
    return this.clients.clientFor(service, () => this.transport());
  }

  /**
   * A connection to one session on this host, on the page's `Session`-targeted IPC connection to it.
   *
   * The hint's LiveKit fields are not read, and not because they are unsupported: a session on this
   * host is served by this host, and routing it out to a media server and back would make the
   * desktop's own sessions require the thing this stack made optional. So a session reached this way
   * advertises `{"rpc"}` whatever the daemon advertised about it, and no room, participant, token or
   * LiveKit identity exists anywhere on this path.
   *
   * Two attachments of the same session get two connections with two `close()`s, over the one wire
   * the host application holds for it — see {@link IpcSessionWire}.
   */
  openSession(sessionId: string, hint: SessionAttachmentHint): SessionConnection {
    if (hint.sessionId !== sessionId) {
      throw new Error(`openSession(${sessionId}) was given a hint for session ${hint.sessionId}`);
    }
    return openIpcSession(this, this.wireFor(sessionId), sessionId);
  }

  private wireFor(sessionId: string): IpcSessionWire {
    const open = this.sessions.get(sessionId);
    if (open) return open;
    const wire = new IpcSessionWire(sessionId, this.wiring, () => {
      this.sessions.delete(sessionId);
    });
    this.sessions.set(sessionId, wire);
    return wire;
  }
}

/**
 * Where each of one attachment's terminals has got to, looked up by terminal id.
 *
 * A session has several terminals and each is at its own offset, so re-opening one must not resume
 * it from another's. Per attachment, because that is what a screen's scrollback belongs to.
 */
function terminalResumePoints(): (terminalId: string) => TerminalResumePoint {
  const points = new Map<string, TerminalResumePoint>();
  return (terminalId: string) => {
    const existing = points.get(terminalId);
    if (existing) return existing;
    const fresh = new TerminalResumePoint();
    points.set(terminalId, fresh);
    return fresh;
  };
}

/**
 * One attachment to `sessionId`, over the wire this page holds for it.
 *
 * The attachment is what `close()` ends; whether that also ends the wire is the wire's business.
 */
function openIpcSession(
  host: IpcHostConnection,
  wire: IpcSessionWire,
  sessionId: string,
): SessionConnection {
  wire.attach();
  let attached = true;
  const refuseIfClosed = () => {
    if (!attached) throw new Error(`session ${sessionId} on host ${host.hostId} is closed`);
  };

  const resumePointFor = terminalResumePoints();

  return {
    hostId: host.hostId,
    sessionId,
    get status(): ConnectionStatus {
      return attached ? wire.status : "idle";
    },
    get error(): string | null {
      return attached ? wire.error : null;
    },
    capabilities: IPC_CAPABILITIES,
    clientFor<S extends DescService>(service: S): Client<S> {
      refuseIfClosed();
      return wire.clientFor(service);
    },
    transport(): Transport {
      refuseIfClosed();
      return wire.transport();
    },
    close(): void {
      if (!attached) return;
      attached = false;
      wire.detach();
    },
    /**
     * The terminal over the *host's* `ConnectionService`, not this session's connection.
     *
     * The daemon holds the capture ring, so scrollback and the offset-anchored resume come from it —
     * the same reason `openHostServedSession` and the LiveKit session connection both ask the host
     * for history rather than the session.
     */
    openTerminal(options: TerminalOptions): TerminalFeed {
      refuseIfClosed();
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
  const resolved: ResolvedWiring = {
    ipc: wiring.ipc ?? thisPagesIpcHost(),
    hostTransport: wiring.hostTransport,
    transportFor: wiring.transportFor,
  };
  // One connection for the host, so `clientFor` is stable for as long as this provider is
  // registered — the same guarantee `LiveKitConnectionProvider` makes, for the same reason.
  let connection: HostConnection | null = null;

  return {
    id: IPC_PROVIDER_ID,
    connectHost(asked: string): HostConnection | null {
      // A registration naming no instance claims nothing: a provider that answered to `""` would
      // shadow every other wire for a host id nobody has.
      if (!hostId || asked !== hostId) return null;
      connection ??= new IpcHostConnection(hostId, resolved);
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
 * Registered **behind** the LiveKit source and ahead of the serving one — see the ordering note in
 * `../selectedDaemon`'s `useDirectorySources`. The plan had it in front, on the expectation that it
 * would be the richer account of the machine; it is not, because `GetClientConfig` carries an
 * instance id and neither `repos_base_path` nor `max_attachment_bytes`, so in front it would shadow
 * a common room's advertisement with a poorer copy and cost the local host its attachment cap. Its
 * `sourceId` is therefore for diagnostics — which account of a host the directory kept — rather than
 * for winning.
 *
 * `idle` with no hosts when the daemon named no instance — a page served by something that is not a
 * daemon has no local host to offer, exactly as `useServingHostDirectorySource` reports.
 */
export function createLocalHostDirectorySource(
  registration: LocalHostRegistration,
): HostDirectorySource {
  const hostId = registration.daemonInstanceId.trim();
  if (!hostId) return { id: LOCAL_IPC_SOURCE_ID, status: "idle", error: null, hosts: [] };
  return {
    id: LOCAL_IPC_SOURCE_ID,
    status: "connected",
    error: null,
    hosts: [
      hostDescriptorOf({ instanceId: hostId, label: registration.label }, LOCAL_IPC_SOURCE_ID),
    ],
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
