import { useState } from "react";
import { ProviderKind } from "../../gen/models_pb";

/**
 * The add-provider form. Providers are configured explicitly — nothing is auto-detected — and the
 * API key travels **inbound only**: it is carried by `CreateProvider` and never read back, so the
 * form clears it on success and the provider list can only ever report *whether* a credential is
 * stored (docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md § AC6).
 */

/** The provider kinds a daemon can talk to, in the order the form offers them. */
export const PROVIDER_KIND_OPTIONS: ReadonlyArray<{ kind: ProviderKind; label: string }> = [
  { kind: ProviderKind.OLLAMA, label: "Ollama" },
  { kind: ProviderKind.OPENAI, label: "OpenAI" },
  { kind: ProviderKind.FIREWORKS, label: "Fireworks" },
  { kind: ProviderKind.ANTHROPIC, label: "Anthropic" },
];

const fieldClassName =
  "rounded border border-input bg-background px-2 py-1 text-sm text-foreground";

export interface AddProviderFormProps {
  /** Resolves to the error to show, or `""` once the provider exists on the daemon. */
  onSubmit: (input: {
    kind: ProviderKind;
    label: string;
    baseUrl: string;
    apiKey: string;
  }) => Promise<string>;
  onDone: () => void;
}

export function AddProviderForm({ onSubmit, onDone }: AddProviderFormProps) {
  const [kind, setKind] = useState<ProviderKind>(ProviderKind.OLLAMA);
  const [label, setLabel] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [error, setError] = useState("");

  const submit = async () => {
    const failure = await onSubmit({
      kind,
      label: label.trim(),
      baseUrl: baseUrl.trim(),
      apiKey: apiKey.trim(),
    });
    setError(failure);
    if (failure === "") {
      setLabel("");
      setBaseUrl("");
      setApiKey("");
      onDone();
    }
  };

  return (
    <div
      data-testid="models-add-provider-form"
      className="mt-3 flex flex-col gap-2 rounded-md border border-border p-3"
    >
      <select
        data-testid="models-add-provider-kind"
        className={fieldClassName}
        value={String(kind)}
        onChange={(e) => setKind(Number(e.target.value) as ProviderKind)}
      >
        {PROVIDER_KIND_OPTIONS.map((option) => (
          <option key={option.kind} value={String(option.kind)}>
            {option.label}
          </option>
        ))}
      </select>
      <input
        data-testid="models-add-provider-label"
        placeholder="Label"
        className={fieldClassName}
        value={label}
        onChange={(e) => setLabel(e.target.value)}
      />
      <input
        data-testid="models-add-provider-base-url"
        placeholder="Base URL"
        className={fieldClassName}
        value={baseUrl}
        onChange={(e) => setBaseUrl(e.target.value)}
      />
      <input
        data-testid="models-add-provider-api-key"
        type="password"
        placeholder="API key (stored on the daemon, never read back)"
        className={fieldClassName}
        value={apiKey}
        onChange={(e) => setApiKey(e.target.value)}
      />
      {error ? (
        <div data-testid="models-add-provider-error" className="text-xs text-destructive">
          {error}
        </div>
      ) : null}
      <button
        type="button"
        data-testid="models-add-provider-submit"
        className="self-start rounded-md border border-input px-3 py-1 text-sm font-medium text-foreground hover:bg-accent"
        onClick={() => void submit()}
      >
        Add provider
      </button>
    </div>
  );
}
