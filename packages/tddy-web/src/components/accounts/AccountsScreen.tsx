/**
 * Accounts screen — every credential a daemon holds, grouped by provider.
 *
 * Presentational: it renders what it is given, matching the `HostsScreen` / `HostsAppPage` split.
 * `AccountsAppPage` owns the `ListAccounts` call and the four outcomes it can come back with.
 *
 * **No row ever carries a secret.** `accounts.proto` has no field to put one in; `hasSecret` is the
 * single bit that tells a linked account from a stale row.
 */

import { useState } from "react";
import { Button } from "../ui/button";

/**
 * `#keyring` 6/9's aggregate sync standing for one account, worst peer first. `null` means
 * nothing has synced the account yet — no peer configured to receive credentials, or none
 * admitted — and renders no badge at all, which is the common case for a daemon with no
 * `keyring.group_secret` configured.
 */
export type AccountSyncStatus = "synced" | "pending" | "undeliverable" | "conflict" | "refused";

export interface AccountRow {
  /** Stable within its provider, and what `#keyring` 5/9 assigns to a project. Never renamed. */
  accountId: string;
  /** What a human calls this account — the only mutable field. */
  label: string;
  /** The provider's own identifier, shown beside the label. Never a credential. */
  subject: string;
  updatedAtUnixSeconds: bigint;
  /** Whether a usable credential is present. One bit, not the secret. */
  hasSecret: boolean;
  /** See [`AccountSyncStatus`]. */
  syncStatus: AccountSyncStatus | null;
}

export interface ProviderGroup {
  provider: string;
  accounts: AccountRow[];
}

/**
 * What the daemon came back with. The four are held apart deliberately: an open-and-empty vault, no
 * vault yet, a vault that exists but is not unlocked on this daemon, and a read that failed are
 * different facts with different remedies — choose a passphrase, enter the passphrase, fix the
 * daemon. Rendering any two of them the same way would tell a person to re-link accounts they
 * already have.
 */
export type AccountsOutcome =
  | { kind: "listed"; providers: ProviderGroup[] }
  | { kind: "uninitialized" }
  | { kind: "locked" }
  | { kind: "error"; reason: string };

export interface AccountsScreenProps {
  outcome: AccountsOutcome;
  onRename: (provider: string, accountId: string, label: string) => void;
  onRemove: (provider: string, accountId: string) => void;
}

export function AccountsScreen({ outcome, onRename, onRemove }: AccountsScreenProps) {
  return <div data-testid="accounts-screen">{renderOutcome(outcome, onRename, onRemove)}</div>;
}

function renderOutcome(
  outcome: AccountsOutcome,
  onRename: AccountsScreenProps["onRename"],
  onRemove: AccountsScreenProps["onRemove"],
) {
  switch (outcome.kind) {
    case "uninitialized":
      return (
        <div data-testid="accounts-uninitialized" className="text-sm space-y-1">
          <p className="font-medium">You have no credential vault on this daemon yet.</p>
          <p className="text-muted-foreground">
            Linked accounts are kept in a vault sealed under a passphrase you choose. Choose one to
            create it: the dashboard asks for it in a prompt over every screen.
          </p>
          <PromptWhereabouts />
        </div>
      );
    case "locked":
      return (
        <div data-testid="accounts-locked" className="text-sm space-y-1">
          <p className="font-medium">Your credential vault is locked on this daemon.</p>
          <p className="text-muted-foreground">
            Unlock it with your passphrase to see your accounts; nothing in it is lost. The dashboard
            asks for the passphrase in a prompt over every screen.
          </p>
          <PromptWhereabouts />
        </div>
      );
    case "error":
      return (
        <p data-testid="accounts-error" className="text-sm text-destructive">
          {outcome.reason}
        </p>
      );
    case "listed":
      if (outcome.providers.length === 0) {
        return (
          <p data-testid="accounts-empty" className="text-muted-foreground text-sm">
            No accounts are linked on this daemon yet. An account appears here once you link it
            from the provider it belongs to.
          </p>
        );
      }
      return (
        <div className="space-y-6">
          {outcome.providers.map((group) => (
            <section key={group.provider} data-testid={`accounts-group-${group.provider}`}>
              <h2 className="text-sm font-semibold mb-2">{group.provider}</h2>
              <ul className="space-y-2">
                {group.accounts.map((account) => (
                  <AccountRowView
                    key={account.accountId}
                    provider={group.provider}
                    account={account}
                    onRename={onRename}
                    onRemove={onRemove}
                  />
                ))}
              </ul>
            </section>
          ))}
        </div>
      );
  }
}

/**
 * The passphrase is asked for by the app-wide `CredentialVaultPrompt`, which opens by itself while
 * the vault is locked or not created — this screen points at it rather than asking a second time.
 * Its "Not now" hides it until the page next loads, so that is the way back to it.
 */
function PromptWhereabouts() {
  return (
    <p className="text-muted-foreground">
      If you closed that prompt with “Not now”, reload the page to see it again.
    </p>
  );
}

interface AccountRowViewProps {
  provider: string;
  account: AccountRow;
  onRename: AccountsScreenProps["onRename"];
  onRemove: AccountsScreenProps["onRemove"];
}

function AccountRowView({ provider, account, onRename, onRemove }: AccountRowViewProps) {
  const rowId = `accounts-row-${provider}-${account.accountId}`;
  const [draftLabel, setDraftLabel] = useState(account.label);
  // Removal is irreversible from this screen, so the first press only asks.
  const [confirmingRemoval, setConfirmingRemoval] = useState(false);

  return (
    <li data-testid={rowId} className="flex flex-wrap items-center gap-3 text-sm">
      <span data-testid={`${rowId}-label`} className="font-medium">
        {account.label}
      </span>
      <span data-testid={`${rowId}-subject`} className="text-muted-foreground">
        {account.subject}
      </span>
      {account.hasSecret ? null : (
        <span className="text-amber-700 text-xs">no credential stored</span>
      )}
      <SyncStatusBadge rowId={rowId} status={account.syncStatus} />
      <form
        className="flex items-center gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          onRename(provider, account.accountId, draftLabel);
        }}
      >
        <input
          data-testid={`${rowId}-rename-input`}
          aria-label={`New label for ${account.label}`}
          className="h-7 rounded-md border border-border bg-background px-2 text-sm"
          value={draftLabel}
          onChange={(event) => setDraftLabel(event.target.value)}
        />
        <Button type="submit" size="sm" variant="outline" data-testid={`${rowId}-rename-submit`}>
          Rename
        </Button>
      </form>
      {confirmingRemoval ? (
        <span className="flex items-center gap-2">
          <span>Remove this account's credential?</span>
          <Button
            size="sm"
            variant="destructive"
            data-testid={`${rowId}-remove-confirm`}
            onClick={() => {
              setConfirmingRemoval(false);
              onRemove(provider, account.accountId);
            }}
          >
            Remove
          </Button>
          <Button size="sm" variant="ghost" onClick={() => setConfirmingRemoval(false)}>
            Cancel
          </Button>
        </span>
      ) : (
        <Button
          size="sm"
          variant="ghost"
          data-testid={`${rowId}-remove`}
          onClick={() => setConfirmingRemoval(true)}
        >
          Remove…
        </Button>
      )}
    </li>
  );
}

/** Text and color per [`AccountSyncStatus`] — worst (`refused`) reads the most alarming. */
const SYNC_STATUS_PRESENTATION: Record<AccountSyncStatus, { label: string; className: string }> = {
  synced: { label: "Synced", className: "text-emerald-700" },
  pending: { label: "Pending", className: "text-muted-foreground" },
  undeliverable: { label: "Undeliverable", className: "text-amber-700" },
  conflict: { label: "Conflict", className: "text-amber-700" },
  refused: { label: "Refused", className: "text-destructive" },
};

/**
 * `#keyring` 6/9's aggregate sync badge. Renders nothing at all for `null` — a daemon with no
 * `keyring.group_secret` configured, the common case — rather than an empty or zero-value badge.
 */
function SyncStatusBadge({ rowId, status }: { rowId: string; status: AccountSyncStatus | null }) {
  if (status === null) return null;
  const { label, className } = SYNC_STATUS_PRESENTATION[status];
  return (
    <span data-testid={`${rowId}-sync-status`} className={`text-xs ${className}`}>
      {label}
    </span>
  );
}
