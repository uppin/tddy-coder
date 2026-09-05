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
  livekitUrl?: string;
  livekitRoom?: string;
  commonRoom?: string;
  daemonMode?: boolean;
  daemonInstanceId?: string;
  allowedAgents?: ClientAllowedAgent[];
  debug?: string;
}

/** The JSON `GET /api/config` serves — snake_case, as `tddy_coder::web_server::ClientConfig`. */
interface ClientConfigJson {
  livekit_url?: string;
  livekit_room?: string;
  common_room?: string;
  daemon_mode?: boolean;
  daemon_instance_id?: string;
  allowed_agents?: ClientAllowedAgent[];
  debug?: string;
}

function fromJson(json: ClientConfigJson): ClientConfig {
  return {
    livekitUrl: json.livekit_url,
    livekitRoom: json.livekit_room,
    commonRoom: json.common_room,
    daemonMode: json.daemon_mode,
    daemonInstanceId: json.daemon_instance_id,
    allowedAgents: json.allowed_agents,
    debug: json.debug,
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
    livekitUrl: response.livekitUrl,
    livekitRoom: response.livekitRoom,
    commonRoom: response.commonRoom,
    daemonMode: response.daemonMode,
    daemonInstanceId: response.daemonInstanceId,
    allowedAgents: response.allowedAgents.map(({ id, label }) => ({ id, label })),
    debug: response.debug,
  };
}
