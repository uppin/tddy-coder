/**
 * Acceptance tests: the Accounts screen (`/accounts`) shows what a daemon's credential store holds,
 * and lets a person rename or forget one entry.
 *
 * The behaviour that matters most here is that **three outcomes stay three outcomes**. A vault that
 * opened and holds nothing, a vault whose key no longer unwraps, and a read that failed are
 * different facts with different recoveries; collapsing any pair of them would tell a person to
 * re-link accounts they already have.
 *
 * The second thing pinned here is a negative: **no secret reaches the screen**. `accounts.proto`
 * has no field to carry one, so the assertion is over the rendered document.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-09-19-keyring-accounts.md
 */

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
    listAccounts: () => ({ providers, vaultLocked: false }),
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
    accountsScreenPage.group("cloudflare").find('[data-testid="accounts-row-github-ada"]').should("not.exist");
  });

  it("shows the provider's own identifier beside the label a person chose", () => {
    mountAccounts(aBackendListing([{ provider: "github", accounts: [ADA] }]));

    accountsScreenPage.subject("github", "ada").should("contain.text", "ada-lovelace");
  });

  it("says a vault that opened and holds nothing is empty", () => {
    mountAccounts(aBackendListing([]));

    accountsScreenPage.emptyNotice().should("exist");
    accountsScreenPage.lockedNotice().should("not.exist");
    accountsScreenPage.errorNotice().should("not.exist");
  });

  it("says a vault is locked rather than showing it as empty", () => {
    mountAccounts(
      anInMemoryRpcBackend().implement(AccountsService, {
        listAccounts: () => ({ providers: [], vaultLocked: true }),
      }),
    );

    accountsScreenPage.lockedNotice().should("exist");
    accountsScreenPage.emptyNotice().should("not.exist");
  });

  it("shows the reason a store could not be read, rather than an empty list", () => {
    mountAccounts(
      anInMemoryRpcBackend().implement(AccountsService, {
        listAccounts: () => {
          throw new Error("the vault file is truncated");
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

  it("removes an account through RemoveAccount once the removal is confirmed", () => {
    const asked: { provider: string; accountId: string }[] = [];
    mountAccounts(
      anInMemoryRpcBackend().implement(AccountsService, {
        listAccounts: () => ({
          providers: [{ provider: "github", accounts: [ADA, BOB] }],
          vaultLocked: false,
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
