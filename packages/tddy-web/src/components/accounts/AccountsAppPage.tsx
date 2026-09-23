/**
 * Data container for the Accounts screen: one `ListAccounts` call against the selected daemon.
 *
 * One RPC, deliberately — a vault's contents change only when somebody links, renames or removes an
 * account, and each of those is an action this screen already knows it took. A rename answers with
 * the account as it now stands and a removal with what remains, so neither re-reads.
 */

import { useEffect, useState } from "react";
import { ConnectError } from "@connectrpc/connect";
import {
  AccountsService,
  type AccountSummary,
  type ProviderAccounts,
} from "../../gen/accounts_pb";
import { useAuthContext } from "../../hooks/authProvider";
import { useDaemonClient } from "../../rpc/selectedDaemon";
import { AppShell } from "../shell/AppShell";
import {
  AccountsScreen,
  type AccountRow,
  type AccountsOutcome,
  type ProviderGroup,
} from "./AccountsScreen";

function rowFromRpc(account: AccountSummary): AccountRow {
  return {
    accountId: account.accountId,
    label: account.label,
    subject: account.subject,
    updatedAtUnixSeconds: account.updatedAt,
    hasSecret: account.hasSecret,
  };
}

function groupsFromRpc(providers: ProviderAccounts[]): ProviderGroup[] {
  return providers.map((group) => ({
    provider: group.provider,
    accounts: group.accounts.map(rowFromRpc),
  }));
}

/** The daemon's reason, verbatim — without the transport's `[code]` prefix. */
function reasonOf(error: unknown): string {
  return ConnectError.from(error).rawMessage;
}

/** `providers` with one account's row replaced by `renamed`. */
function withRenamed(providers: ProviderGroup[], provider: string, renamed: AccountRow) {
  return providers.map((group) =>
    group.provider !== provider
      ? group
      : {
          ...group,
          accounts: group.accounts.map((account) =>
            account.accountId === renamed.accountId ? renamed : account,
          ),
        },
  );
}

export function AccountsAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  const { sessionToken } = useAuthContext();
  const client = useDaemonClient(AccountsService);

  const [outcome, setOutcome] = useState<AccountsOutcome | null>(null);
  // A failed rename or removal is reported beside the list rather than replacing it: the list the
  // person was looking at is still what the vault holds.
  const [actionError, setActionError] = useState<string | null>(null);

  // One read per visit; it re-fires only when the selected daemon or the session token changes,
  // and each of those genuinely invalidates the answer (see `HostsAppPage`).
  useEffect(() => {
    if (!client) return;
    let current = true;
    client
      .listAccounts({ sessionToken: sessionToken ?? "" })
      .then((res) => {
        if (!current) return;
        setOutcome(
          res.vaultLocked
            ? { kind: "locked" }
            : { kind: "listed", providers: groupsFromRpc(res.providers) },
        );
      })
      .catch((e: unknown) => {
        if (!current) return;
        setOutcome({ kind: "error", reason: reasonOf(e) });
      });
    return () => {
      current = false;
    };
  }, [client, sessionToken]);

  const rename = (provider: string, accountId: string, label: string) => {
    if (!client) return;
    client
      .setAccountLabel({ sessionToken: sessionToken ?? "", provider, accountId, label })
      .then((res) => {
        const renamed = res.account;
        if (!renamed) return;
        setActionError(null);
        setOutcome((previous) =>
          previous?.kind === "listed"
            ? {
                kind: "listed",
                providers: withRenamed(previous.providers, provider, rowFromRpc(renamed)),
              }
            : previous,
        );
      })
      .catch((e: unknown) => setActionError(reasonOf(e)));
  };

  const remove = (provider: string, accountId: string) => {
    if (!client) return;
    client
      .removeAccount({ sessionToken: sessionToken ?? "", provider, accountId })
      .then((res) => {
        setActionError(null);
        setOutcome({ kind: "listed", providers: groupsFromRpc(res.providers) });
      })
      .catch((e: unknown) => setActionError(reasonOf(e)));
  };

  return (
    <AppShell title="Accounts" onNavigate={onNavigate} dataTestId="accounts-app-page">
      {actionError ? (
        <p className="mb-3 text-sm text-destructive" data-testid="accounts-action-error">
          {actionError}
        </p>
      ) : null}
      {outcome ? <AccountsScreen outcome={outcome} onRename={rename} onRemove={remove} /> : null}
    </AppShell>
  );
}
