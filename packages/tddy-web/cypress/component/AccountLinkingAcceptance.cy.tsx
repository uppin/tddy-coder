/**
 * Acceptance tests: adding a *second* account at a provider, from the Accounts screen.
 *
 * **The load-bearing test here is a negative one.** The convenient implementation calls the login
 * flow `#keyring` 2/9 already built and throws away the session it hands back — and that passes
 * every positive test on this page. It also signs the person in as the account they just added, so
 * a person linking their work GitHub would find themselves looking at a different vault. The test
 * that catches it is `keeps the session on the account it was established with`: the marker must
 * still be on the account the person arrived as.
 *
 * The second thing pinned here is that a link's four end states stay four. A refusal at the
 * provider, an expired code and a vault this session cannot open need different words in front of a
 * person, and two of them are not failures of anything the person did.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-09-19-keyring-link-github.md
 */

import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AccountsAppPage } from "../../src/components/accounts/AccountsAppPage";
import {
  AccountsService,
  LinkState,
  type AccountSummary,
  type ProviderAccounts,
} from "../../src/gen/accounts_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { accountsScreenPage } from "../support/pages/accountsScreenPage";

const UPDATED_AT = 1_726_700_000n;
const USER_CODE = "WXYZ-1234";
const VERIFICATION_URI = "https://github.com/login/device";

function anAccount(overrides: Partial<AccountSummary>): AccountSummary {
  return {
    provider: "github",
    accountId: "account-ada",
    label: "Ada at work",
    subject: "ada",
    updatedAt: UPDATED_AT,
    hasSecret: true,
    ...overrides,
  } as AccountSummary;
}

/** The account the session was established with — the one a person signed in as. */
const ADA = anAccount({ accountId: "account-ada", label: "Ada at work", subject: "ada" });
/** A second account at the same provider, added by linking. */
const GRACE = anAccount({ accountId: "account-grace", label: "Grace", subject: "grace" });

const SESSION_IS_ADA = { provider: "github", accountId: "account-ada" };

function githubHolding(accounts: AccountSummary[]): ProviderAccounts[] {
  return [{ provider: "github", accounts } as ProviderAccounts];
}

/**
 * A daemon whose vault holds Ada, and whose provider answers a link with `state`.
 *
 * The listing grows Grace once a link has been reported linked, which is what a daemon does: the
 * record is in the vault by the time the poll answers.
 */
function aDaemonWhereALinkEndsAs(state: LinkState): InMemoryRpcBackend {
  let linked = false;
  return anInMemoryRpcBackend().implement(AccountsService, {
    listAccounts: () => ({
      providers: githubHolding(linked ? [ADA, GRACE] : [ADA]),
      vaultLocked: false,
      sessionAccount: SESSION_IS_ADA,
    }),
    beginLinkAccount: () => ({
      linkId: "link-1",
      userCode: USER_CODE,
      verificationUri: VERIFICATION_URI,
      expiresInSeconds: 900n,
      intervalSeconds: 5n,
    }),
    pollLinkAccount: () => {
      linked = state === LinkState.LINK_LINKED;
      return { state, account: GRACE, intervalSeconds: 5n };
    },
  });
}

function mountAccounts(backend: InMemoryRpcBackend) {
  mountWithRpc(withSelectedDaemon(<AccountsAppPage onNavigate={() => {}} />), backend);
}

describe("Linking a second account", () => {
  it("offers to add another account at a provider the vault already holds one at", () => {
    mountAccounts(aDaemonWhereALinkEndsAs(LinkState.LINK_PENDING));

    accountsScreenPage.addAccountButton("github").should("exist");
  });

  it("shows the operator the code to type and where to type it", () => {
    mountAccounts(aDaemonWhereALinkEndsAs(LinkState.LINK_PENDING));

    accountsScreenPage.addAccountButton("github").click();

    accountsScreenPage.linkUserCode().should("contain.text", USER_CODE);
    accountsScreenPage.linkVerificationUri().should("have.attr", "href", VERIFICATION_URI);
  });

  it("asks the daemon to begin a link at the provider whose control was pressed", () => {
    const asked: string[] = [];
    mountAccounts(
      anInMemoryRpcBackend().implement(AccountsService, {
        listAccounts: () => ({
          providers: githubHolding([ADA]),
          vaultLocked: false,
          sessionAccount: SESSION_IS_ADA,
        }),
        beginLinkAccount: (request) => {
          asked.push(request.provider);
          return {
            linkId: "link-1",
            userCode: USER_CODE,
            verificationUri: VERIFICATION_URI,
            expiresInSeconds: 900n,
            intervalSeconds: 5n,
          };
        },
      }),
    );

    accountsScreenPage.addAccountButton("github").click();

    cy.wrap(asked).should("deep.equal", ["github"]);
  });

  it("adds the linked account to the provider's group", () => {
    mountAccounts(aDaemonWhereALinkEndsAs(LinkState.LINK_LINKED));

    accountsScreenPage.addAccountButton("github").click();

    accountsScreenPage.row("github", "account-grace").should("exist");
    accountsScreenPage.label("github", "account-grace").should("contain.text", "Grace");
  });

  it("keeps the session on the account it was established with", () => {
    mountAccounts(aDaemonWhereALinkEndsAs(LinkState.LINK_LINKED));

    accountsScreenPage.addAccountButton("github").click();

    accountsScreenPage.row("github", "account-grace").should("exist");
    accountsScreenPage.sessionMarker("github", "account-ada").should("exist");
    accountsScreenPage.sessionMarker("github", "account-grace").should("not.exist");
  });

  it("marks the account this session belongs to", () => {
    mountAccounts(aDaemonWhereALinkEndsAs(LinkState.LINK_PENDING));

    accountsScreenPage.sessionMarker("github", "account-ada").should("exist");
  });

  it("marks no account when the session belongs to none of them", () => {
    mountAccounts(
      anInMemoryRpcBackend().implement(AccountsService, {
        listAccounts: () => ({ providers: githubHolding([ADA, GRACE]), vaultLocked: false }),
      }),
    );

    accountsScreenPage.sessionMarker("github", "account-ada").should("not.exist");
    accountsScreenPage.sessionMarker("github", "account-grace").should("not.exist");
  });

  it("offers no way to forget the account the session was established with", () => {
    mountAccounts(aDaemonWhereALinkEndsAs(LinkState.LINK_PENDING));

    accountsScreenPage.removeButton("github", "account-ada").should("not.exist");
  });

  it("still offers to forget every other account", () => {
    mountAccounts(
      anInMemoryRpcBackend().implement(AccountsService, {
        listAccounts: () => ({
          providers: githubHolding([ADA, GRACE]),
          vaultLocked: false,
          sessionAccount: SESSION_IS_ADA,
        }),
      }),
    );

    accountsScreenPage.removeButton("github", "account-grace").should("exist");
  });

  it("says the operator refused, rather than reporting a failure", () => {
    mountAccounts(aDaemonWhereALinkEndsAs(LinkState.LINK_DENIED));

    accountsScreenPage.addAccountButton("github").click();

    accountsScreenPage.linkDeniedNotice().should("exist");
    accountsScreenPage.linkExpiredNotice().should("not.exist");
    accountsScreenPage.linkLockedNotice().should("not.exist");
  });

  it("says a code outlived its window, rather than that the operator refused", () => {
    mountAccounts(aDaemonWhereALinkEndsAs(LinkState.LINK_EXPIRED));

    accountsScreenPage.addAccountButton("github").click();

    accountsScreenPage.linkExpiredNotice().should("exist");
    accountsScreenPage.linkDeniedNotice().should("not.exist");
  });

  it("says an approval had nowhere to go, rather than that the operator refused", () => {
    mountAccounts(aDaemonWhereALinkEndsAs(LinkState.LINK_VAULT_LOCKED));

    accountsScreenPage.addAccountButton("github").click();

    accountsScreenPage.linkLockedNotice().should("exist");
    accountsScreenPage.linkDeniedNotice().should("not.exist");
  });

  it("leaves the code on screen while nobody has approved yet", () => {
    mountAccounts(aDaemonWhereALinkEndsAs(LinkState.LINK_PENDING));

    accountsScreenPage.addAccountButton("github").click();

    accountsScreenPage.linkUserCode().should("contain.text", USER_CODE);
    accountsScreenPage.linkDeniedNotice().should("not.exist");
    accountsScreenPage.linkExpiredNotice().should("not.exist");
  });
});
