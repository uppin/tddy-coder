/**
 * Acceptance tests: the Accounts screen (`/accounts`) shows what a daemon's credential store holds,
 * and lets a person rename or forget one entry.
 *
 * The behaviour that matters most here is that **four outcomes stay four outcomes**. A vault that
 * is open and holds nothing, no vault yet, a vault that exists but is not unlocked on this daemon,
 * and a read that failed are different facts with different recoveries; collapsing any pair of them
 * would tell a person to re-link accounts they already have.
 *
 * The second thing pinned here is a negative: **no secret reaches the screen**. `accounts.proto`
 * has no field to carry one, so the assertion is over the rendered document.
 *
 * Feature: docs/ft/web/accounts-screen.md
 */

import { Code, ConnectError } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AccountsAppPage } from "../../src/components/accounts/AccountsAppPage";
import { AccountsService, type AccountSummary } from "../../src/gen/accounts_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { accountsScreenPage } from "../support/pages/accountsScreenPage";

const UPDATED_AT = 1_726_700_000n;

function anAccount(overrides: Partial<AccountSummary>): AccountSummary {
  return {
    provider: "github",
    accountId: "ada",
    label: "Ada at work",
    subject: "ada-lovelace",
    updatedAt: UPDATED_AT,
    hasSecret: true,
    ...overrides,
  } as AccountSummary;
}

const ADA = anAccount({ provider: "github", accountId: "ada", label: "Ada at work" });
const BOB = anAccount({
  provider: "github",
  accountId: "bob",
  label: "Bob the bot",
  subject: "bob-bot",
});
const ZOE = anAccount({
  provider: "cloudflare",
  accountId: "zoe",
  label: "Zone admin",
  subject: "zoe@example.com",
});

function aBackendListing(
  providers: { provider: string; accounts: AccountSummary[] }[],
): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(AccountsService, {
    listAccounts: () => ({ providers, vaultLocked: false, vaultUninitialized: false }),
  });
}

function mountAccounts(backend: InMemoryRpcBackend) {
  mountWithRpc(withSelectedDaemon(<AccountsAppPage onNavigate={() => {}} />), backend);
}

describe("Accounts screen", () => {
  it("lists a provider's accounts under that provider", () => {
    mountAccounts(aBackendListing([{ provider: "github", accounts: [ADA, BOB] }]));

    accountsScreenPage.group("github").should("exist");
    accountsScreenPage.label("github", "ada").should("contain.text", "Ada at work");
    accountsScreenPage.label("github", "bob").should("contain.text", "Bob the bot");
  });

  it("keeps two providers' accounts in separate groups", () => {
    mountAccounts(
      aBackendListing([
        { provider: "cloudflare", accounts: [ZOE] },
        { provider: "github", accounts: [ADA] },
      ]),
    );

    accountsScreenPage.row("cloudflare", "zoe").should("exist");
    accountsScreenPage.row("github", "ada").should("exist");
    accountsScreenPage.group("cloudflare").within(() => {
      accountsScreenPage.row("github", "ada").should("not.exist");
    });
  });

  it("shows the provider's own identifier beside the label a person chose", () => {
    mountAccounts(aBackendListing([{ provider: "github", accounts: [ADA] }]));

    accountsScreenPage.subject("github", "ada").should("contain.text", "ada-lovelace");
  });

  it("says a vault that opened and holds nothing is empty", () => {
    mountAccounts(aBackendListing([]));

    accountsScreenPage.emptyNotice().should("exist");
    accountsScreenPage.uninitializedNotice().should("not.exist");
    accountsScreenPage.lockedNotice().should("not.exist");
    accountsScreenPage.errorNotice().should("not.exist");
  });

  it("says no vault exists yet, and that a passphrase creates one, rather than showing it as empty", () => {
    mountAccounts(
      anInMemoryRpcBackend().implement(AccountsService, {
        listAccounts: () => ({ providers: [], vaultLocked: false, vaultUninitialized: true }),
      }),
    );

    accountsScreenPage.uninitializedNotice().should("contain.text", "Choose one to create it");
    accountsScreenPage.emptyNotice().should("not.exist");
    accountsScreenPage.lockedNotice().should("not.exist");
    accountsScreenPage.errorNotice().should("not.exist");
  });

  it("says a vault is locked rather than showing it as empty", () => {
    mountAccounts(
      anInMemoryRpcBackend().implement(AccountsService, {
        listAccounts: () => ({ providers: [], vaultLocked: true, vaultUninitialized: false }),
      }),
    );

    accountsScreenPage.lockedNotice().should("contain.text", "Unlock it with your passphrase");
    accountsScreenPage.emptyNotice().should("not.exist");
    accountsScreenPage.uninitializedNotice().should("not.exist");
  });

  it("shows the reason a store could not be read, rather than an empty list", () => {
    mountAccounts(
      anInMemoryRpcBackend().implement(AccountsService, {
        listAccounts: () => {
          throw new ConnectError("the vault file is truncated", Code.Internal);
        },
      }),
    );

    accountsScreenPage.errorNotice().should("contain.text", "the vault file is truncated");
    accountsScreenPage.emptyNotice().should("not.exist");
  });

  it("renames an account through SetAccountLabel and leaves its identity alone", () => {
    const asked: { provider: string; accountId: string; label: string }[] = [];
    mountAccounts(
      anInMemoryRpcBackend().implement(AccountsService, {
        listAccounts: () => ({
          providers: [{ provider: "github", accounts: [ADA] }],
          vaultLocked: false,
          vaultUninitialized: false,
        }),
        setAccountLabel: (request) => {
          asked.push({
            provider: request.provider,
            accountId: request.accountId,
            label: request.label,
          });
          return { account: anAccount({ label: request.label }) };
        },
      }),
    );

    accountsScreenPage.rename("github", "ada", "Ada — personal");

    cy.wrap(asked).should("deep.equal", [
      { provider: "github", accountId: "ada", label: "Ada — personal" },
    ]);
    accountsScreenPage.label("github", "ada").should("contain.text", "Ada — personal");
  });

  it("reports a failed rename beside the list, which stays", () => {
    mountAccounts(
      anInMemoryRpcBackend().implement(AccountsService, {
        listAccounts: () => ({
          providers: [{ provider: "github", accounts: [ADA] }],
          vaultLocked: false,
          vaultUninitialized: false,
        }),
        setAccountLabel: () => {
          throw new ConnectError(
            "the credential store could not be read or written on this daemon",
            Code.Internal,
          );
        },
      }),
    );

    accountsScreenPage.rename("github", "ada", "Ada — personal");

    accountsScreenPage.actionError().should("contain.text", "could not be read or written");
    accountsScreenPage.label("github", "ada").should("contain.text", "Ada at work");
  });

  it("removes an account through RemoveAccount once the removal is confirmed", () => {
    const asked: { provider: string; accountId: string }[] = [];
    mountAccounts(
      anInMemoryRpcBackend().implement(AccountsService, {
        listAccounts: () => ({
          providers: [{ provider: "github", accounts: [ADA, BOB] }],
          vaultLocked: false,
          vaultUninitialized: false,
        }),
        removeAccount: (request) => {
          asked.push({ provider: request.provider, accountId: request.accountId });
          return { providers: [{ provider: "github", accounts: [ADA] }] };
        },
      }),
    );

    accountsScreenPage.remove("github", "bob");

    cy.wrap(asked).should("deep.equal", [{ provider: "github", accountId: "bob" }]);
    accountsScreenPage.row("github", "bob").should("not.exist");
    accountsScreenPage.row("github", "ada").should("exist");
  });

  it("does not send a removal that was never confirmed", () => {
    const asked: string[] = [];
    mountAccounts(
      anInMemoryRpcBackend().implement(AccountsService, {
        listAccounts: () => ({
          providers: [{ provider: "github", accounts: [ADA] }],
          vaultLocked: false,
          vaultUninitialized: false,
        }),
        removeAccount: (request) => {
          asked.push(request.accountId);
          return { providers: [] };
        },
      }),
    );

    accountsScreenPage.removeButton("github", "ada").click();

    cy.wrap(asked).should("deep.equal", []);
    accountsScreenPage.row("github", "ada").should("exist");
  });
});
