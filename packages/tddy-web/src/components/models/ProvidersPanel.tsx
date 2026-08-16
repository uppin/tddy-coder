import { useState } from "react";
import type { ProviderKind } from "../../gen/models_pb";
import { providerRowKey, type ProviderRow } from "../../utils/mergeRegistryEntries";
import {
  AddProviderForm,
  PROVIDER_KIND_OPTIONS,
  type AddProviderFormProps,
} from "./AddProviderForm";

/**
 * The providers every connected daemon is configured with, each reporting whether a credential is
 * stored for it (never the credential) and the error its last enumeration produced, if any — a
 * provider that cannot be enumerated says so instead of showing a stale or invented model list
 * (docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md § AC6, AC7).
 */

function kindLabel(kind: ProviderKind): string {
  return PROVIDER_KIND_OPTIONS.find((option) => option.kind === kind)?.label ?? "Unknown";
}

/**
 * The `data-testid` stem of a provider row. Provider ids are minted per daemon — `prov-ollama`
 * exists on every host — so the owning daemon is part of the id, or two hosts' rows would be
 * indistinguishable in the DOM.
 */
export function providerRowTestId(provider: ProviderRow): string {
  return `models-provider-row-${provider.daemonInstanceId}-${provider.providerId}`;
}

export interface ProvidersPanelProps {
  providers: ProviderRow[];
  /**
   * Enumeration errors by {@link providerRowKey} — the daemon's own, plus any refresh this screen
   * triggered.
   */
  providerErrors: ReadonlyMap<string, string>;
  onAddProvider: AddProviderFormProps["onSubmit"];
  onRefreshProvider: (provider: ProviderRow) => void;
}

export function ProvidersPanel({
  providers,
  providerErrors,
  onAddProvider,
  onRefreshProvider,
}: ProvidersPanelProps) {
  const [addOpen, setAddOpen] = useState(false);

  return (
    <section data-testid="models-providers-panel" className="mb-6">
      <h2 className="mb-2 text-sm font-semibold text-foreground">Providers</h2>
      <button
        type="button"
        data-testid="models-add-provider-toggle"
        className="rounded-md border border-input px-3 py-1 text-sm font-medium text-foreground hover:bg-accent"
        onClick={() => setAddOpen((open) => !open)}
      >
        Add provider
      </button>
      {addOpen ? (
        <AddProviderForm onSubmit={onAddProvider} onDone={() => setAddOpen(false)} />
      ) : null}

      <div className="mt-3 flex flex-col gap-2">
        {providers.map((provider) => {
          const error = providerErrors.get(providerRowKey(provider)) ?? "";
          const testId = providerRowTestId(provider);
          return (
            <div
              key={providerRowKey(provider)}
              data-testid={testId}
              className="rounded-md border border-border p-3 text-sm text-foreground"
            >
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-medium">{provider.label}</span>
                <span className="text-xs text-muted-foreground">{kindLabel(provider.kind)}</span>
                <span className="text-xs text-muted-foreground">{provider.baseUrl}</span>
                <span className="text-xs text-muted-foreground">{provider.daemonInstanceId}</span>
                <span
                  data-testid={`${testId}-credential`}
                  data-has-credential={String(provider.hasCredential)}
                  className="text-xs text-muted-foreground"
                >
                  {provider.hasCredential ? "Credential stored" : "No credential"}
                </span>
                <button
                  type="button"
                  data-testid={`${testId}-refresh`}
                  className="rounded-md border border-input px-2 py-1 text-xs font-medium text-foreground hover:bg-accent"
                  onClick={() => onRefreshProvider(provider)}
                >
                  Refresh models
                </button>
              </div>
              {error ? (
                <div
                  data-testid={`${testId}-error`}
                  className="mt-1 text-xs text-destructive"
                >
                  {error}
                </div>
              ) : null}
            </div>
          );
        })}
      </div>
    </section>
  );
}
