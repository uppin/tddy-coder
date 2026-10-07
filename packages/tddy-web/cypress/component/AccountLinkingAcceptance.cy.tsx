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

import { useState } from "react";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AccountsAppPage } from "../../src/components/accounts/AccountsAppPage";
import {
  MAX_CONSECUTIVE_POLL_FAILURES,
  MIN_POLL_INTERVAL_SECONDS,
} from "../../src/components/accounts/useLinkFlow";
import { AccountsService, LinkState } from "../../src/gen/accounts_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import {
  ADA,
  DAEMON_FAILURE_REASON,
  GRACE,
  LINK_ID,
  LINK_INTERVAL_MS,
  LINK_USER_CODE,
  LINK_VERIFICATION_URI,
  POLL_FAILS,
  SESSION_IS_ADA,
  aDaemonWherePollsAre,
  githubHolding,
  type LinkingDaemon,
} from "../support/rpc/linkingBackend";
import { accountsScreenPage } from "../support/pages/accountsScreenPage";

function mountAccounts(daemon: LinkingDaemon) {
  mountWithRpc(withSelectedDaemon(<AccountsAppPage onNavigate={() => {}} />), daemon.backend);
}

/** The page with a way to leave it, so a spec can unmount it mid-attempt. */
function AccountsPageThatCanBeLeft() {
  const [here, setHere] = useState(true);
  return (
    <>
      {here ? <AccountsAppPage onNavigate={() => {}} /> : null}
      <button data-testid="leave-the-page" onClick={() => setHere(false)}>
        leave
      </button>
    </>
  );
}

/** Press Add and wait until the code is on screen: the attempt has begun, no poll has run. */
function pressAddAccount() {
  accountsScreenPage.addAccountButton("github").click();
  accountsScreenPage.linkUserCode().should("contain.text", LINK_USER_CODE);
}

function letOnePollIntervalPass() {
  cy.tick(LINK_INTERVAL_MS);
}

/**
 * Let the `poll`th poll run and its answer be handled. The next poll is scheduled only once the
 * answer has been handled, so a spec that ticks again straight away would tick past a timer that
 * does not exist yet.
 */
function letPollNumberRun(daemon: LinkingDaemon, poll: number) {
  letOnePollIntervalPass();
  expectPollCount(daemon, poll);
  cy.wait(0);
}

function letEveryAllowedPollFail(daemon: LinkingDaemon) {
  for (let poll = 1; poll <= MAX_CONSECUTIVE_POLL_FAILURES; poll += 1) {
    letPollNumberRun(daemon, poll);
  }
}

function expectPollCount(daemon: LinkingDaemon, count: number) {
  cy.wrap(daemon.polls).should("have.length", count);
}

/** Press Add and let the first poll run, which is where an attempt's end state arrives. */
function linkUntilThePollAnswers(daemon: LinkingDaemon) {
  cy.clock();
  mountAccounts(daemon);
  pressAddAccount();
  letOnePollIntervalPass();
}

describe("Linking a second account", () => {
  describe("beginning an attempt", () => {
    it("offers to add another account at a provider the vault already holds one at", () => {
      mountAccounts(aDaemonWherePollsAre([LinkState.LINK_PENDING]));

      accountsScreenPage.addAccountButton("github").should("exist");
    });

    it("shows the operator the code to type and where to type it", () => {
      mountAccounts(aDaemonWherePollsAre([LinkState.LINK_PENDING]));

      accountsScreenPage.addAccountButton("github").click();

      accountsScreenPage.linkUserCode().should("contain.text", LINK_USER_CODE);
      accountsScreenPage.linkVerificationUri().should("have.attr", "href", LINK_VERIFICATION_URI);
    });

    it("asks the daemon to begin a link at the provider whose control was pressed", () => {
      const daemon = aDaemonWherePollsAre([LinkState.LINK_PENDING]);
      mountAccounts(daemon);

      accountsScreenPage.addAccountButton("github").click();

      cy.wrap(daemon.begins).should("deep.equal", ["github"]);
    });

    it("reports a begin that failed with the daemon's reason and shows no code", () => {
      const daemon = aDaemonWherePollsAre([LinkState.LINK_PENDING], { beginFails: true });
      cy.clock();
      mountAccounts(daemon);

      accountsScreenPage.addAccountButton("github").click();
      letOnePollIntervalPass();

      accountsScreenPage.actionError().should("contain.text", DAEMON_FAILURE_REASON);
      accountsScreenPage.linkUserCode().should("not.exist");
      expectPollCount(daemon, 0);
    });

    it("starts one attempt when Add is pressed twice in a row", () => {
      const daemon = aDaemonWherePollsAre([LinkState.LINK_PENDING]);
      cy.clock();
      mountAccounts(daemon);

      accountsScreenPage.addAccountButton("github").dblclick();
      accountsScreenPage.linkUserCode().should("contain.text", LINK_USER_CODE);
      letOnePollIntervalPass();

      cy.wrap(daemon.begins).should("have.length", 1);
      expectPollCount(daemon, 1);
    });
  });

  describe("polling", () => {
    it("has not polled before the interval the provider named has passed", () => {
      const daemon = aDaemonWherePollsAre([LinkState.LINK_PENDING]);
      cy.clock();
      mountAccounts(daemon);
      pressAddAccount();
      expectPollCount(daemon, 0);

      cy.tick(LINK_INTERVAL_MS - 1);

      expectPollCount(daemon, 0);
    });

    it("polls the attempt it began once the interval has passed", () => {
      const daemon = aDaemonWherePollsAre([LinkState.LINK_PENDING]);
      cy.clock();
      mountAccounts(daemon);
      pressAddAccount();
      cy.tick(LINK_INTERVAL_MS - 1);
      expectPollCount(daemon, 0);

      cy.tick(1);

      cy.wrap(daemon.polls).should("deep.equal", [LINK_ID]);
    });

    it("polls again, one interval later, while nobody has approved yet", () => {
      const daemon = aDaemonWherePollsAre([LinkState.LINK_PENDING]);
      cy.clock();
      mountAccounts(daemon);
      pressAddAccount();
      letPollNumberRun(daemon, 1);

      letPollNumberRun(daemon, 2);
    });

    it("waits at least the minimum when the daemon names an interval of zero", () => {
      const daemon = aDaemonWherePollsAre([LinkState.LINK_PENDING], { intervalSeconds: 0n });
      cy.clock();
      mountAccounts(daemon);
      pressAddAccount();
      const minimumMs = MIN_POLL_INTERVAL_SECONDS * 1000;

      cy.tick(minimumMs - 1);
      expectPollCount(daemon, 0);
      cy.tick(1);

      expectPollCount(daemon, 1);
    });

    it("stops polling once the page has been left", () => {
      const daemon = aDaemonWherePollsAre([LinkState.LINK_PENDING]);
      cy.clock();
      mountWithRpc(withSelectedDaemon(<AccountsPageThatCanBeLeft />), daemon.backend);
      pressAddAccount();

      cy.get('[data-testid="leave-the-page"]').click();
      letOnePollIntervalPass();

      expectPollCount(daemon, 0);
    });
  });

  describe("when a poll fails in transit", () => {
    it("keeps the attempt and polls again after one failed poll", () => {
      const daemon = aDaemonWherePollsAre([POLL_FAILS, LinkState.LINK_PENDING]);
      cy.clock();
      mountAccounts(daemon);
      pressAddAccount();

      letPollNumberRun(daemon, 1);
      letPollNumberRun(daemon, 2);

      accountsScreenPage.linkUserCode().should("contain.text", LINK_USER_CODE);
      accountsScreenPage.actionError().should("not.exist");
    });

    it("links the account when the retry after a failed poll is approved", () => {
      const daemon = aDaemonWherePollsAre([POLL_FAILS, LinkState.LINK_LINKED]);
      cy.clock();
      mountAccounts(daemon);
      pressAddAccount();

      letPollNumberRun(daemon, 1);
      letPollNumberRun(daemon, 2);

      accountsScreenPage.row("github", GRACE.accountId).should("exist");
    });

    it("gives up with the daemon's reason after the allowed consecutive failures", () => {
      const daemon = aDaemonWherePollsAre([POLL_FAILS]);
      cy.clock();
      mountAccounts(daemon);
      pressAddAccount();

      letEveryAllowedPollFail(daemon);

      accountsScreenPage.actionError().should("contain.text", DAEMON_FAILURE_REASON);
      accountsScreenPage.linkUserCode().should("not.exist");
    });

    it("stops polling once it has given up", () => {
      const daemon = aDaemonWherePollsAre([POLL_FAILS]);
      cy.clock();
      mountAccounts(daemon);
      pressAddAccount();
      letEveryAllowedPollFail(daemon);
      expectPollCount(daemon, MAX_CONSECUTIVE_POLL_FAILURES);

      letOnePollIntervalPass();

      expectPollCount(daemon, MAX_CONSECUTIVE_POLL_FAILURES);
    });
  });

  describe("when the account is linked", () => {
    it("adds the linked account to the provider's group", () => {
      linkUntilThePollAnswers(aDaemonWherePollsAre([LinkState.LINK_LINKED]));

      accountsScreenPage.row("github", GRACE.accountId).should("exist");
      accountsScreenPage.label("github", GRACE.accountId).should("contain.text", GRACE.label);
    });

    it("keeps the session marker on the account the session was established with", () => {
      linkUntilThePollAnswers(aDaemonWherePollsAre([LinkState.LINK_LINKED]));

      accountsScreenPage.row("github", GRACE.accountId).should("exist");
      accountsScreenPage.sessionMarker("github", ADA.accountId).should("exist");
      accountsScreenPage.sessionMarker("github", GRACE.accountId).should("not.exist");
    });

    it("sends the session token the page was reading with on every request, and signs nobody in", () => {
      const daemon = aDaemonWherePollsAre([LinkState.LINK_LINKED]);
      linkUntilThePollAnswers(daemon);
      accountsScreenPage.row("github", GRACE.accountId).should("exist");

      cy.wrap(daemon.sessionTokensSeen).should((tokens) => {
        const [tokenOfTheFirstListing] = tokens;
        expect(tokens, "list, begin, poll, list again").to.have.length.at.least(4);
        expect(tokens.every((token) => token === tokenOfTheFirstListing)).to.equal(true);
      });
      cy.wrap(null).should(() => expect(daemon.sessionEstablishingCalls()).to.equal(0));
    });
  });

  describe("how the session's own account is shown", () => {
    it("marks the account this session belongs to", () => {
      mountAccounts(aDaemonWherePollsAre([LinkState.LINK_PENDING]));

      accountsScreenPage.sessionMarker("github", ADA.accountId).should("exist");
    });

    it("marks no account when the session belongs to none of them", () => {
      mountWithRpc(
        withSelectedDaemon(<AccountsAppPage onNavigate={() => {}} />),
        anInMemoryRpcBackend().implement(AccountsService, {
          listAccounts: () => ({ providers: githubHolding([ADA, GRACE]), vaultLocked: false }),
        }),
      );

      accountsScreenPage.sessionMarker("github", ADA.accountId).should("not.exist");
      accountsScreenPage.sessionMarker("github", GRACE.accountId).should("not.exist");
    });

    it("offers no way to forget the account the session was established with", () => {
      mountAccounts(aDaemonWherePollsAre([LinkState.LINK_PENDING]));

      accountsScreenPage.removeButton("github", ADA.accountId).should("not.exist");
    });

    it("still offers to forget every other account", () => {
      mountWithRpc(
        withSelectedDaemon(<AccountsAppPage onNavigate={() => {}} />),
        anInMemoryRpcBackend().implement(AccountsService, {
          listAccounts: () => ({
            providers: githubHolding([ADA, GRACE]),
            vaultLocked: false,
            sessionAccount: SESSION_IS_ADA,
          }),
        }),
      );

      accountsScreenPage.removeButton("github", GRACE.accountId).should("exist");
    });
  });

  describe("how an attempt ends without linking", () => {
    it("says the operator refused, rather than reporting a failure", () => {
      linkUntilThePollAnswers(aDaemonWherePollsAre([LinkState.LINK_DENIED]));

      accountsScreenPage.linkDeniedNotice().should("exist");
      accountsScreenPage.linkExpiredNotice().should("not.exist");
      accountsScreenPage.linkLockedNotice().should("not.exist");
    });

    it("says a code outlived its window, rather than that the operator refused", () => {
      linkUntilThePollAnswers(aDaemonWherePollsAre([LinkState.LINK_EXPIRED]));

      accountsScreenPage.linkExpiredNotice().should("exist");
      accountsScreenPage.linkDeniedNotice().should("not.exist");
    });

    it("says an approval had nowhere to go, rather than that the operator refused", () => {
      linkUntilThePollAnswers(aDaemonWherePollsAre([LinkState.LINK_VAULT_LOCKED]));

      accountsScreenPage.linkLockedNotice().should("exist");
      accountsScreenPage.linkDeniedNotice().should("not.exist");
    });

    it("leaves the code on screen while nobody has approved yet", () => {
      linkUntilThePollAnswers(aDaemonWherePollsAre([LinkState.LINK_PENDING]));

      accountsScreenPage.linkUserCode().should("contain.text", LINK_USER_CODE);
      accountsScreenPage.linkDeniedNotice().should("not.exist");
      accountsScreenPage.linkExpiredNotice().should("not.exist");
    });
  });
});
