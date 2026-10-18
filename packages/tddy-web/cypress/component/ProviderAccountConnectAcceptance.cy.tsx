/**
 * Acceptance tests: connecting a model-provider account (Meta, Fireworks, …) by pasting its API key
 * on the Accounts screen.
 *
 * The daemon checks the key against the provider before storing anything, so the screen has two
 * outcomes to tell apart: the account joins its provider's group, or the daemon's reason is shown
 * and nothing is added. The key is typed into a password field and never appears on screen again —
 * `accounts.proto` has no field to send it back in.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-10-18-provider-route-accounts.md
 */

import { Code, ConnectError } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AccountsAppPage } from "../../src/components/accounts/AccountsAppPage";
import {
  AccountsService,
  SyncStatus,
  type AccountSummary,
  type ConnectProviderAccountRequest,
} from "../../src/gen/accounts_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { recordedFields } from "../support/rpc/recordedRequests";
import { accountsScreenPage } from "../support/pages/accountsScreenPage";

const A_META_KEY = "meta-key-that-must-never-be-echoed";

const META_ACCOUNT = {
  provider: "meta",
  accountId: "meta-hackathon",
  label: "Meta hackathon",
  subject: "",
  updatedAt: 1_760_000_000n,
  hasSecret: true,
  syncStatus: SyncStatus.UNSPECIFIED,
  preset: "meta",
  wireProtocols: ["anthropic-messages", "openai-responses", "openai-chat"],
} as AccountSummary;

/** A daemon whose vault starts empty and which accepts or refuses the connect as told. */
function aDaemon(answer: "accepts" | { refusesWith: string }) {
  const connects: ConnectProviderAccountRequest[] = [];
  let connected = false;
  const backend = anInMemoryRpcBackend().implement(AccountsService, {
    listAccounts: () => ({
      providers: connected ? [{ provider: "meta", accounts: [META_ACCOUNT] }] : [],
      vaultLocked: false,
      vaultUninitialized: false,
    }),
    connectProviderAccount: (request: ConnectProviderAccountRequest) => {
      connects.push(request);
      if (answer !== "accepts") {
        throw new ConnectError(answer.refusesWith, Code.InvalidArgument);
      }
      connected = true;
      return { account: META_ACCOUNT };
    },
  });
  return { backend, connects };
}

function mountAccounts(backend: ReturnType<typeof aDaemon>["backend"]) {
  mountWithRpc(withSelectedDaemon(<AccountsAppPage onNavigate={() => {}} />), backend);
}

describe("Connecting a provider account by API key", () => {
  it("adds a Meta account to its provider group", () => {
    const daemon = aDaemon("accepts");
    mountAccounts(daemon.backend);

    accountsScreenPage.connectProviderAccount("meta", "Meta hackathon", A_META_KEY);

    accountsScreenPage.label("meta", "meta-hackathon").should("contain.text", "Meta hackathon");
    accountsScreenPage.presetBadge("meta", "meta-hackathon").should("contain.text", "meta");
    cy.wrap(daemon.connects)
      .should("have.length", 1)
      .then(() => {
        const [sent] = recordedFields(daemon.connects);
        expect(sent.preset).to.equal("meta");
        expect(sent.label).to.equal("Meta hackathon");
        expect(sent.apiKey).to.equal(A_META_KEY);
      });
  });

  it("shows the daemon's reason when the provider rejects the key, and adds nothing", () => {
    const daemon = aDaemon({ refusesWith: "the provider refused the key: 401 invalid API key" });
    mountAccounts(daemon.backend);

    accountsScreenPage.connectProviderAccount("meta", "Typo", "not-the-key");

    accountsScreenPage
      .connectError()
      .should("contain.text", "the provider refused the key: 401 invalid API key");
    accountsScreenPage.group("meta").should("not.exist");
  });

  it("asks for an endpoint URL only for presets whose endpoint the operator supplies", () => {
    mountAccounts(aDaemon("accepts").backend);

    accountsScreenPage.connectApiKeyButton().click();
    accountsScreenPage.connectPreset().select("meta");
    accountsScreenPage.connectOpenAiBaseUrl().should("not.exist");
    accountsScreenPage.connectPreset().select("modal");
    accountsScreenPage.connectOpenAiBaseUrl().should("be.visible");
  });

  it("never shows the key once it has been sent", () => {
    const daemon = aDaemon("accepts");
    mountAccounts(daemon.backend);

    accountsScreenPage.connectProviderAccount("meta", "Meta hackathon", A_META_KEY);

    accountsScreenPage.label("meta", "meta-hackathon").should("exist");
    cy.get("body").should("not.contain.text", A_META_KEY);
  });
});
