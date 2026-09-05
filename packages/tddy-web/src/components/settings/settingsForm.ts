/**
 * Between the daemon's settings and the form that edits them.
 *
 * Two rules live here, and both are hazards if they leak into the component. The daemon never
 * returns its API secret, so the form holds only whether one is stored, and an update that leaves
 * the secret field blank must omit the field rather than send an empty string — which would clear
 * the daemon's credentials. And an update carries the *complete* settings message, so every field
 * the form holds goes back, not only the edited one.
 */

import { create } from "@bufbuild/protobuf";
import {
  DaemonSettingsSchema,
  type DaemonSettings,
  type GetConfigResponse,
} from "../../gen/daemon_config_pb";

export interface DaemonSettingsFormState {
  livekitUrl: string;
  livekitPublicUrl: string;
  livekitApiKey: string;
  /** What the operator typed. Blank means "leave the stored secret alone". */
  livekitApiSecret: string;
  livekitCommonRoom: string;
  /** True when the daemon holds a secret. Display only — never the secret itself. */
  livekitApiSecretSet: boolean;
  webPort: string;
  webHost: string;
}

/** The form as the daemon's current configuration describes it. */
export function toFormState(response: GetConfigResponse): DaemonSettingsFormState {
  // A daemon with no `livekit:` block has no LiveKit settings at all, which reads as blank fields
  // rather than as an empty server address.
  const livekit = response.settings?.livekit;
  const listen = response.settings?.listen;
  return {
    livekitUrl: livekit?.url ?? "",
    livekitPublicUrl: livekit?.publicUrl ?? "",
    livekitApiKey: livekit?.apiKey ?? "",
    // The daemon reports that a secret exists, never the secret — so there is nothing to fill in.
    livekitApiSecret: "",
    livekitCommonRoom: livekit?.commonRoom ?? "",
    livekitApiSecretSet: livekit?.apiSecretSet ?? false,
    webPort: listen?.webPort === undefined ? "" : String(listen.webPort),
    webHost: listen?.webHost ?? "",
  };
}

/** The complete settings message an update carries back. */
export function toUpdateSettings(form: DaemonSettingsFormState): DaemonSettings {
  return create(DaemonSettingsSchema, {
    livekit: {
      url: form.livekitUrl,
      publicUrl: form.livekitPublicUrl,
      apiKey: form.livekitApiKey,
      // A blank field means "leave the stored secret alone". Sending it as an empty string would
      // clear the daemon's credentials, so the field is left out of the message entirely.
      apiSecret: form.livekitApiSecret === "" ? undefined : form.livekitApiSecret,
      commonRoom: form.livekitCommonRoom,
      // `api_secret_set` is read-only: the daemon ignores it on update.
    },
    listen: {
      webPort: form.webPort === "" ? undefined : Number(form.webPort),
      webHost: form.webHost,
    },
  });
}
