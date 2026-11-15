/**
 * Acceptance tests: connecting a Coder deployment on the Accounts screen by entering its URL and a
 * session token.
 *
 * The daemon asks the deployment who the token belongs to before storing anything, so the screen
 * has two outcomes to tell apart: the account joins the Coder group under the deployment's
 * username, or the daemon's reason is shown beside the form and what was typed stays. The token is
 * typed into a password field and never appears on screen again — `accounts.proto` has no field to
 * send it back in.
 *
 * PRD: docs/ft/daemon/provider-accounts.md
 */

import { Code, ConnectError } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AccountsContent } from "../../src/accounts/AccountsContent";
import {
  AccountsService,
  SyncStatus,
  type AccountSummary,
  type ConnectCoderAccountRequest,
} from "tddy-client-web/gen/accounts_pb";
import { recordedFields } from "../support/rpc/recordedRequests";
import { accountsScreenPage } from "../support/pages/accountsScreenPage";

const THE_DEPLOYMENT = "https://coder.example.com";
const A_CODER_TOKEN = "coder-token-that-must-never-be-echoed";
const ADAS_ACCOUNT_ID = "coder-3f1c9a52-7d0e-4b7a-9a43-0c2f8f1f6b10@coder.example.com";

const ADAS_CODER_ACCOUNT = {
  provider: "coder",
  accountId: ADAS_ACCOUNT_ID,
  label: "Work Coder",
  subject: "ada-lovelace",
  updatedAt: 1_760_000_000n,
  hasSecret: true,
  syncStatus: SyncStatus.UNSPECIFIED,
  preset: "",
  wireProtocols: [],
  defaultFor: [],
} as AccountSummary;

/** A daemon whose vault starts empty and which accepts or refuses the Coder connect as told. */
function aDaemon(answer: "accepts" | { refusesWith: string }) {
  const connects: ConnectCoderAccountRequest[] = [];
  let connected = false;
  const backend = anInMemoryRpcBackend().implement(AccountsService, {
    listAccounts: () => ({
      providers: connected ? [{ provider: "coder", accounts: [ADAS_CODER_ACCOUNT] }] : [],
      vaultLocked: false,
      vaultUninitialized: false,
    }),
    connectCoderAccount: (request: ConnectCoderAccountRequest) => {
      connects.push(request);
      if (answer !== "accepts") {
        throw new ConnectError(answer.refusesWith, Code.Unauthenticated);
      }
      connected = true;
      return { account: ADAS_CODER_ACCOUNT };
    },
  });
  return { backend, connects };
}

function mountAccounts(backend: ReturnType<typeof aDaemon>["backend"]) {
  cy.mountWithClients(<AccountsContent />, backend);
}

describe("Connecting a Coder deployment", () => {
  it("connects a Coder deployment and shows it under the Coder group", () => {
    const daemon = aDaemon("accepts");
    mountAccounts(daemon.backend);

    accountsScreenPage.connectCoderAccount(THE_DEPLOYMENT, A_CODER_TOKEN, "Work Coder");

    accountsScreenPage
      .group("coder")
      .find(`[data-testid="accounts-row-coder-${ADAS_ACCOUNT_ID}"]`)
      .should("exist");
    accountsScreenPage.label("coder", ADAS_ACCOUNT_ID).should("contain.text", "Work Coder");
    accountsScreenPage.subject("coder", ADAS_ACCOUNT_ID).should("contain.text", "ada-lovelace");
    cy.wrap(daemon.connects)
      .should("have.length", 1)
      .then(() => {
        const [sent] = recordedFields(daemon.connects);
        expect(sent.deploymentUrl).to.equal(THE_DEPLOYMENT);
        expect(sent.token).to.equal(A_CODER_TOKEN);
        expect(sent.label).to.equal("Work Coder");
      });
  });

  it("shows the daemon's refusal beside the form and keeps what was typed", () => {
    const daemon = aDaemon({
      refusesWith: "the deployment rejected this token: 401 API key is invalid",
    });
    mountAccounts(daemon.backend);

    accountsScreenPage.connectCoderAccount(THE_DEPLOYMENT, "not-the-token", "Work Coder");

    accountsScreenPage
      .coderConnectError()
      .should("contain.text", "the deployment rejected this token: 401 API key is invalid");
    accountsScreenPage.coderUrl().should("have.value", THE_DEPLOYMENT);
    accountsScreenPage.coderToken().should("have.value", "not-the-token");
    accountsScreenPage.coderLabel().should("have.value", "Work Coder");
    accountsScreenPage.row("coder", ADAS_ACCOUNT_ID).should("not.exist");
  });

  it("never renders the token after the connect", () => {
    const daemon = aDaemon("accepts");
    mountAccounts(daemon.backend);

    accountsScreenPage.connectCoderAccount(THE_DEPLOYMENT, A_CODER_TOKEN, "Work Coder");

    accountsScreenPage.row("coder", ADAS_ACCOUNT_ID).should("exist");
    cy.get("body").should("not.contain.text", A_CODER_TOKEN);
    cy.get("input").should(($inputs) => {
      const held = [...$inputs].map((input) => (input as HTMLInputElement).value);
      expect(held).not.to.include(A_CODER_TOKEN);
    });
  });
});
