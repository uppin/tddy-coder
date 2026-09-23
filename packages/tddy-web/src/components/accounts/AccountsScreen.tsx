/**
 * Accounts screen — every credential a daemon holds, grouped by provider.
 *
 * Presentational: it renders what it is given, matching the `HostsScreen` / `HostsAppPage` split.
 * `AccountsAppPage` owns the `ListAccounts` call and the three outcomes it can come back with.
 *
 * **No row ever carries a secret.** `accounts.proto` has no field to put one in; `hasSecret` is the
 * single bit that tells a linked account from a stale row.
 */

import { useState } from "react";
import { Button } from "../ui/button";

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
}

export interface ProviderGroup {
  provider: string;
  accounts: AccountRow[];
}

/**
 * What the daemon came back with. The three are held apart deliberately: an opened-and-empty vault,
 * a vault whose key no longer unwraps, and a read that failed are different facts, and rendering
 * any two of them the same way would tell a person to re-link accounts they already have.
 */
export type AccountsOutcome =
  | { kind: "listed"; providers: ProviderGroup[] }
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
    case "locked":
      return (
        <div data-testid="accounts-locked" className="text-sm space-y-1">
          <p className="font-medium">This daemon's credential store is locked.</p>
          <p className="text-muted-foreground">
            It was sealed under a different sign-in, so your current session cannot open it. Nothing
            has been deleted, but the accounts it holds cannot be read — re-link them to recover.
          </p>
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
