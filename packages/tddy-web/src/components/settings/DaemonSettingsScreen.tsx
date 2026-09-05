/**
 * The daemon's own settings.
 *
 * The desktop application is a daemon with a UI, so the configuration that used to be reachable
 * only by editing YAML and restarting is edited here: read through `DaemonConfigService.GetConfig`,
 * written through `UpdateConfig`. What the daemon cannot apply to itself while running, it names in
 * the response, and this screen says so rather than leaving the operator to wonder.
 */

import { useEffect, useState } from "react";
import { ConnectError, type Client } from "@connectrpc/connect";
import type { DaemonConfigService } from "../../gen/daemon_config_pb";
import { toFormState, toUpdateSettings, type DaemonSettingsFormState } from "./settingsForm";

export interface DaemonSettingsScreenProps {
  client: Client<typeof DaemonConfigService>;
  /** Daemon access token, gating every configuration read and write. */
  sessionToken: string;
}

const inputClass = "w-full rounded border border-border px-2 py-1 text-sm";
const labelClass = "block text-sm mb-1 text-muted-foreground";

export function DaemonSettingsScreen({ client, sessionToken }: DaemonSettingsScreenProps) {
  // Null until the daemon has answered: a blank form would read as a daemon with no configuration.
  const [form, setForm] = useState<DaemonSettingsFormState | null>(null);
  const [configPath, setConfigPath] = useState("");
  const [restartRequired, setRestartRequired] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let cancelled = false;
    client
      .getConfig({ sessionToken })
      .then((response) => {
        if (cancelled) return;
        setForm(toFormState(response));
        setConfigPath(response.configPath);
      })
      .catch((err) => {
        if (!cancelled) setError(ConnectError.from(err).rawMessage);
      });
    return () => {
      cancelled = true;
    };
  }, [client, sessionToken]);

  const edit = (field: keyof DaemonSettingsFormState, value: string) =>
    setForm((current) => (current ? { ...current, [field]: value } : current));

  const save = async () => {
    if (!form) return;
    setSaving(true);
    try {
      const response = await client.updateConfig({
        sessionToken,
        settings: toUpdateSettings(form),
      });
      setRestartRequired(response.restartRequired);
      setError(null);
    } catch (err) {
      // The form keeps what was entered: a refused value is the one the operator has to correct.
      setError(ConnectError.from(err).rawMessage);
      setRestartRequired([]);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div data-testid="daemon-settings-screen" className="flex flex-col gap-4 p-4">
      <div>
        <div className="font-semibold">Daemon settings</div>
        {configPath ? (
          <div className="text-sm text-muted-foreground">{configPath}</div>
        ) : null}
      </div>

      {form === null ? (
        <div className="text-sm text-muted-foreground">Loading the daemon's configuration…</div>
      ) : (
        <>
          <div className="flex max-w-md flex-col gap-3 rounded-md border border-border p-4">
            <div className="font-medium">LiveKit</div>
            <label>
              <span className={labelClass}>URL</span>
              <input
                data-testid="daemon-settings-livekit-url"
                className={inputClass}
                value={form.livekitUrl}
                onChange={(e) => edit("livekitUrl", e.target.value)}
              />
            </label>
            <label>
              <span className={labelClass}>Public URL</span>
              <input
                className={inputClass}
                value={form.livekitPublicUrl}
                onChange={(e) => edit("livekitPublicUrl", e.target.value)}
              />
            </label>
            <label>
              <span className={labelClass}>API key</span>
              <input
                data-testid="daemon-settings-livekit-api-key"
                className={inputClass}
                value={form.livekitApiKey}
                onChange={(e) => edit("livekitApiKey", e.target.value)}
              />
            </label>
            <label>
              <span className={labelClass}>API secret</span>
              <input
                type="password"
                className={inputClass}
                value={form.livekitApiSecret}
                onChange={(e) => edit("livekitApiSecret", e.target.value)}
              />
            </label>
            <div
              data-testid="daemon-settings-livekit-secret-state"
              className="text-sm text-muted-foreground"
            >
              {form.livekitApiSecretSet
                ? "A secret is stored. Type a new one to replace it; leave blank to keep it."
                : "No secret stored."}
            </div>
            <label>
              <span className={labelClass}>Common room</span>
              <input
                className={inputClass}
                value={form.livekitCommonRoom}
                onChange={(e) => edit("livekitCommonRoom", e.target.value)}
              />
            </label>
          </div>

          {/* Where the daemon listens is edited in the YAML file: changing it needs a restart. */}
          <div className="flex max-w-md flex-col gap-1 rounded-md border border-border p-4 text-sm">
            <div className="font-medium">Listen</div>
            <div className="text-muted-foreground">
              Host {form.webHost || "—"}, port {form.webPort || "—"}
            </div>
          </div>

          <button
            type="button"
            data-testid="daemon-settings-save"
            className="self-start rounded-md border border-border px-3 py-2 text-sm font-medium"
            disabled={saving}
            onClick={save}
          >
            Save
          </button>
        </>
      )}

      {error ? (
        <div data-testid="daemon-settings-error" className="text-sm text-destructive">
          {error}
        </div>
      ) : null}

      {restartRequired.length > 0 ? (
        <div data-testid="daemon-settings-restart-required" className="text-sm">
          Saved. A daemon restart is needed to apply: {restartRequired.join(", ")}
        </div>
      ) : null}
    </div>
  );
}
