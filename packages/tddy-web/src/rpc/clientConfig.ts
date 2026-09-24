/**
 * The startup configuration the daemon hands its web bundle.
 *
 * A browser page fetches it from `GET /api/config`, the endpoint the daemon has always served
 * beside the bundle. A page inside the Tauri desktop application was loaded from the asset
 * protocol and has no HTTP origin to fetch from, so it asks the same daemon for the same payload
 * over RPC — `DaemonConfigService.GetClientConfig` is the mirror of that endpoint.
 *
 * Which of the two applies is the transport flavour this page already resolves
 * (`./daemonTransportFlavour`), so the browser dashboard keeps exactly the request it made before.
 */

import { createClient, type Transport } from "@connectrpc/connect";
import { DaemonConfigService } from "../gen/daemon_config_pb";
import { daemonTransportFlavour } from "./daemonTransportFlavour";
import { thisPagesHost, type DaemonHostEnvironment } from "./daemonTransport";

/** An agent the daemon's configuration allows sessions to be started with. */
export interface ClientAllowedAgent {
  id: string;
  label: string;
}

/** The payload, in the one shape both sources are read into. */
export interface ClientConfig {
  /**
   * Whether the serving daemon joins its common room. `false` means the operator switched LiveKit
   * off, so this page builds no `Room` and mints no token — a url and a room name are still
   * present, which is exactly why the flag has to be read rather than inferred from them.
   */
  livekitEnabled?: boolean;
  livekitUrl?: string;
  livekitRoom?: string;
  commonRoom?: string;
  daemonMode?: boolean;
  daemonInstanceId?: string;
  allowedAgents?: ClientAllowedAgent[];
  debug?: string;
  /**
   * What the serving daemon's `--workspace-tools` jail confines, so the Start-Session form can
   * offer the sandboxed-codebase placement on a deployment with no common room to advertise it in.
   *
   * Absent is a host that does not serve the placement — a daemon that predates the key, an OS
   * with no sandbox backend, or a bundle served by something that is not a daemon at all — and the
   * control is disabled with the reason rather than offered. Absence is never read as a default:
   * a daemon that would answer the request field by starting an ordinary, unconfined session is
   * exactly what the disabled state exists to prevent.
   */
  sandboxedCodebase?: { confinesFilesystem: boolean };
  /**
   * What the serving daemon declared about its GitHub sign-in (`auth_flow`). Always stated, so the
   * sign-in screen never has to guess — see {@link AuthFlowDeclaration}.
   */
  authFlow: AuthFlowDeclaration;
}

/** The GitHub sign-in flows a daemon can serve. */
export type AuthFlow = "redirect" | "device";

/**
 * What a daemon declared about its GitHub sign-in.
 *
 * - `"redirect"` — `GetAuthUrl` / `ExchangeCode`, a deployment holding a client secret.
 * - `"device"` — `StartDeviceLogin` / `PollDeviceLogin`, a public client id and no secret.
 * - `"none"` — no `auth_flow` at all: the daemon serves no GitHub sign-in. Never read as a flow.
 * - `{ unrecognised }` — a value this page does not know, kept so the screen can name it rather
 *   than guess a flow the daemon may not serve.
 */
export type AuthFlowDeclaration = AuthFlow | "none" | { unrecognised: string };

/** What a payload's `auth_flow` declares, with absence and unknown values each stated as such. */
function authFlowOf(declared: string | undefined): AuthFlowDeclaration {
  if (declared === undefined) return "none";
  return declared === "redirect" || declared === "device" ? declared : { unrecognised: declared };
}

/** The JSON `GET /api/config` serves — snake_case, as `tddy_coder::web_server::ClientConfig`. */
interface ClientConfigJson {
  livekit_enabled?: boolean;
  livekit_url?: string;
  livekit_room?: string;
  common_room?: string;
  daemon_mode?: boolean;
  daemon_instance_id?: string;
  allowed_agents?: ClientAllowedAgent[];
  debug?: string;
  sandboxed_codebase?: { confines_filesystem?: boolean };
  auth_flow?: string;
}

/**
 * The jail capability a payload described, or `undefined` when it described none.
 *
 * Only an object saying what the jail confines is a capability: an absent key is a host that does
 * not serve the placement, and reading a default here would turn "unadvertised" into a promise.
 * Both sources are narrowed through this one reader, so the browser and the desktop cannot
 * disagree about the same host.
 */
function sandboxedCodebaseOf(
  advertised: boolean,
  confinesFilesystem: boolean | undefined,
): { confinesFilesystem: boolean } | undefined {
  if (!advertised) return undefined;
  return { confinesFilesystem: confinesFilesystem === true };
}

function fromJson(json: ClientConfigJson): ClientConfig {
  return {
    livekitEnabled: json.livekit_enabled,
    livekitUrl: json.livekit_url,
    livekitRoom: json.livekit_room,
    commonRoom: json.common_room,
    daemonMode: json.daemon_mode,
    daemonInstanceId: json.daemon_instance_id,
    allowedAgents: json.allowed_agents,
    debug: json.debug,
    sandboxedCodebase: sandboxedCodebaseOf(
      json.sandboxed_codebase !== undefined && json.sandboxed_codebase !== null,
      json.sandboxed_codebase?.confines_filesystem,
    ),
    authFlow: authFlowOf(json.auth_flow),
  };
}

/**
 * Read the client configuration from the daemon serving this page.
 *
 * Resolves to `null` when the daemon answered but served no configuration — the same "no config,
 * carry on with defaults" outcome a non-OK `/api/config` has always produced. Rejects when the
 * daemon could not be reached at all, which is the caller's cue to fall back to URL parameters.
 *
 * `host` is the injection seam; production passes nothing and this page's own `window` decides.
 */
export async function loadClientConfig(
  transport: Transport,
  host: DaemonHostEnvironment = thisPagesHost(),
): Promise<ClientConfig | null> {
  if (daemonTransportFlavour(host.window) === "http") {
    const response = await fetch("/api/config");
    return response.ok ? fromJson((await response.json()) as ClientConfigJson) : null;
  }

  // `sessionToken` is left unset: the field exists on the request, so the transport's auth gate
  // fills it with a request-time-fresh access token once one is available
  // (see `src/rpc/authGateInterceptor.ts`).
  // The daemon serves this call ungated for exactly that reason — it is read before sign-in, and
  // carries no secrets — so an unfilled token is not a failure here.
  const response = await createClient(DaemonConfigService, transport).getClientConfig({});
  return {
    livekitEnabled: response.livekitEnabled,
    livekitUrl: response.livekitUrl,
    livekitRoom: response.livekitRoom,
    commonRoom: response.commonRoom,
    daemonMode: response.daemonMode,
    daemonInstanceId: response.daemonInstanceId,
    allowedAgents: response.allowedAgents.map(({ id, label }) => ({ id, label })),
    debug: response.debug,
    sandboxedCodebase: sandboxedCodebaseOf(
      response.sandboxedCodebase !== undefined,
      response.sandboxedCodebase?.confinesFilesystem,
    ),
    authFlow: authFlowOf(response.authFlow),
  };
}
