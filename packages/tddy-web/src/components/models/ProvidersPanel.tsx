import { useState } from "react";
import type { ProviderKind } from "../../gen/models_pb";
import { unrecognisedEnumText } from "../../lib/enumSkew";
import { safeTestIdPart } from "../../lib/testId";
import {
  providerRowKey,
  registryEmptyStateText,
  type DaemonFailure,
  type ProviderRow,
  type RegistryReadStatus,
} from "../../utils/mergeRegistryEntries";
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
 *
 * An empty panel is never left to speak for itself: a first-run daemon with nothing configured, a
 * read still in flight and a `ListProviders` that failed all render as no rows, and only the first
 * of the three means "this fleet has no providers".
 */

function kindLabel(kind: ProviderKind): string {
  // A kind this build has no name for — including the unset `PROVIDER_KIND_UNSPECIFIED` — is
  // reported as itself. Rendering "Unknown" would file a daemon newer than this tab, and a daemon
  // that answered without a kind at all, under ordinary data.
  return (
    PROVIDER_KIND_OPTIONS.find((option) => option.kind === kind)?.label ??
    unrecognisedEnumText("provider kind", kind)
  );
}

/**
 * The `data-testid` stem of a provider row. Provider ids are minted per daemon — `prov-ollama`
 * exists on every host — so the owning daemon is part of the id, or two hosts' rows would be
 * indistinguishable in the DOM.
 */
export function providerRowTestId(provider: ProviderRow): string {
  return `models-provider-row-${provider.daemonInstanceId}-${provider.providerId}`;
}

const actionClassName =
  "rounded-md border border-input px-2 py-1 text-xs font-medium text-foreground hover:bg-accent";

export interface ProvidersPanelProps {
  providers: ProviderRow[];
  /**
   * Enumeration errors by {@link providerRowKey} — the daemon's own, plus any refresh this screen
   * triggered.
   */
  providerErrors: ReadonlyMap<string, string>;
  /** Errors from a write against a provider row, by {@link providerRowKey}. */
  providerActionErrors: ReadonlyMap<string, string>;
  /** Daemons whose registry could not be read — an empty panel must not stand for a failed read. */
  failures: DaemonFailure[];
  /** Why the panel is empty, when it is. */
  status: RegistryReadStatus;
  /** The daemon a newly added provider is created on — a provider belongs to exactly one host. */
  addProviderTarget: string;
  onAddProvider: AddProviderFormProps["onSubmit"];
  onRefreshProvider: (provider: ProviderRow) => void;
  onDeleteProvider: (provider: ProviderRow) => void;
}

export function ProvidersPanel({
  providers,
  providerErrors,
  providerActionErrors,
  failures,
  status,
  addProviderTarget,
  onAddProvider,
  onRefreshProvider,
  onDeleteProvider,
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
        <AddProviderForm
          target={addProviderTarget}
          onSubmit={onAddProvider}
          onDone={() => setAddOpen(false)}
        />
      ) : null}

      <div className="mt-3 flex flex-col gap-2">
        {failures.map((failure) => (
          <div
            key={`failure-${failure.instanceId}`}
            data-testid={`models-providers-daemon-error-${safeTestIdPart(failure.instanceId)}`}
            className="rounded-md border border-border p-3 text-sm text-destructive"
          >
            {failure.instanceId}: {failure.error}
          </div>
        ))}
        {providers.map((provider) => {
          const error = providerErrors.get(providerRowKey(provider)) ?? "";
          const actionError = providerActionErrors.get(providerRowKey(provider)) ?? "";
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
                  className={actionClassName}
                  onClick={() => onRefreshProvider(provider)}
                >
                  Refresh models
                </button>
                <button
                  type="button"
                  data-testid={`${testId}-delete`}
                  className={actionClassName}
                  onClick={() => onDeleteProvider(provider)}
                >
                  Delete
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
              {actionError ? (
                <div
                  data-testid={`${testId}-action-error`}
                  className="mt-1 text-xs text-destructive"
                >
                  {actionError}
                </div>
              ) : null}
            </div>
          );
        })}
        {providers.length === 0 && failures.length === 0 ? (
          <div
            data-testid="models-providers-empty"
            data-registry-status={status}
            className="rounded-md border border-border p-3 text-sm text-muted-foreground"
          >
            {registryEmptyStateText(status, {
              loading: "Reading the fleet's providers…",
              ready: "No providers configured",
            })}
          </div>
        ) : null}
      </div>
    </section>
  );
}
