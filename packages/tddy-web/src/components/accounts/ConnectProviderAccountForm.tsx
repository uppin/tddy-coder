/**
 * The "Connect API key" form: a model-provider account (Meta, Fireworks, Baseten, Modal, or a
 * compatible endpoint) added by pasting its key. Signs nobody in.
 *
 * A preset whose endpoint the operator supplies asks for that URL; a fixed preset does not show the
 * field. The key field is a password field and is never read back — `accounts.proto` has no field
 * to return it in.
 */

import { useState } from "react";
import { Button } from "../ui/button";

/** One preset as the form offers it. */
export interface ProviderPresetOption {
  id: string;
  label: string;
  /** The preset needs an OpenAI-compatible root from the operator. */
  needsOpenAiBaseUrl: boolean;
  /** The preset needs an Anthropic Messages root from the operator. */
  needsAnthropicBaseUrl: boolean;
}

/** What the form submits; the container turns it into `ConnectProviderAccount`. */
export interface ConnectProviderAccountInput {
  preset: string;
  label: string;
  apiKey: string;
  openAiBaseUrl: string;
  anthropicBaseUrl: string;
}

export interface ConnectProviderAccountFormProps {
  presets: ProviderPresetOption[];
  /** Set while the daemon is checking the key; the submit control is disabled. */
  pending: boolean;
  /** The daemon's reason the last attempt was refused, verbatim. */
  error: string | null;
  onSubmit: (input: ConnectProviderAccountInput) => void;
  onCancel: () => void;
}

/**
 * The presets the form offers, in the order the daemon's table lists them. Which ones need a URL
 * mirrors `tddy_accounts::provider_account::presets()`; the daemon refuses a request that
 * disagrees, so a drift here shows up as a refusal and never as a wrong endpoint.
 */
export const PROVIDER_PRESET_OPTIONS: ProviderPresetOption[] = [
  { id: "meta", label: "Meta", needsOpenAiBaseUrl: false, needsAnthropicBaseUrl: false },
  { id: "fireworks", label: "Fireworks", needsOpenAiBaseUrl: false, needsAnthropicBaseUrl: false },
  { id: "baseten", label: "Baseten", needsOpenAiBaseUrl: false, needsAnthropicBaseUrl: false },
  { id: "modal", label: "Modal", needsOpenAiBaseUrl: true, needsAnthropicBaseUrl: false },
  {
    id: "openai-compatible",
    label: "OpenAI-compatible",
    needsOpenAiBaseUrl: true,
    needsAnthropicBaseUrl: false,
  },
  {
    id: "anthropic-compatible",
    label: "Anthropic-compatible",
    needsOpenAiBaseUrl: false,
    needsAnthropicBaseUrl: true,
  },
];

const FIELD_CLASS = "h-8 rounded-md border border-border bg-background px-2 text-sm";

export function ConnectProviderAccountForm({
  presets,
  pending,
  error,
  onSubmit,
  onCancel,
}: ConnectProviderAccountFormProps) {
  const [presetId, setPresetId] = useState(presets[0]?.id ?? "");
  const [label, setLabel] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [openAiBaseUrl, setOpenAiBaseUrl] = useState("");
  const [anthropicBaseUrl, setAnthropicBaseUrl] = useState("");
  const preset = presets.find((option) => option.id === presetId);

  return (
    <form
      data-testid="accounts-connect-form"
      className="mt-4 flex max-w-md flex-col gap-3 text-sm"
      onSubmit={(event) => {
        event.preventDefault();
        onSubmit({
          preset: presetId,
          label,
          apiKey,
          // A URL typed for a preset that does not take one is not sent: the daemon refuses it.
          openAiBaseUrl: preset?.needsOpenAiBaseUrl ? openAiBaseUrl : "",
          anthropicBaseUrl: preset?.needsAnthropicBaseUrl ? anthropicBaseUrl : "",
        });
      }}
    >
      <label className="flex flex-col gap-1">
        Provider
        <select
          data-testid="accounts-connect-preset"
          className={FIELD_CLASS}
          value={presetId}
          onChange={(event) => setPresetId(event.target.value)}
        >
          {presets.map((option) => (
            <option key={option.id} value={option.id}>
              {option.label}
            </option>
          ))}
        </select>
      </label>
      <label className="flex flex-col gap-1">
        Label
        <input
          data-testid="accounts-connect-label"
          className={FIELD_CLASS}
          value={label}
          onChange={(event) => setLabel(event.target.value)}
        />
      </label>
      <label className="flex flex-col gap-1">
        API key
        <input
          data-testid="accounts-connect-api-key-input"
          type="password"
          autoComplete="off"
          className={FIELD_CLASS}
          value={apiKey}
          onChange={(event) => setApiKey(event.target.value)}
        />
      </label>
      {preset?.needsOpenAiBaseUrl ? (
        <label className="flex flex-col gap-1">
          OpenAI-compatible base URL
          <input
            data-testid="accounts-connect-openai-base-url"
            className={FIELD_CLASS}
            placeholder="https://host.example/v1"
            value={openAiBaseUrl}
            onChange={(event) => setOpenAiBaseUrl(event.target.value)}
          />
        </label>
      ) : null}
      {preset?.needsAnthropicBaseUrl ? (
        <label className="flex flex-col gap-1">
          Anthropic Messages base URL
          <input
            data-testid="accounts-connect-anthropic-base-url"
            className={FIELD_CLASS}
            placeholder="https://host.example"
            value={anthropicBaseUrl}
            onChange={(event) => setAnthropicBaseUrl(event.target.value)}
          />
        </label>
      ) : null}
      {error ? (
        <p data-testid="accounts-connect-error" className="text-destructive">
          {error}
        </p>
      ) : null}
      <div className="flex gap-2">
        <Button type="submit" size="sm" disabled={pending} data-testid="accounts-connect-submit">
          Connect
        </Button>
        <Button type="button" size="sm" variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </form>
  );
}
