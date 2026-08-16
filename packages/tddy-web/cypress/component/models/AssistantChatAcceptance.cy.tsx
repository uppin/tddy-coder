/**
 * Acceptance: an assistant — a model plus a system prompt plus a tool set — can be chatted with from
 * the Models & Agents screen, and one that has tools is asked **where** they run before the stream
 * opens.
 *
 * That question is not decoration. An assistant's tools execute in the daemon process, so the daemon
 * confines them by path: the ACP `cwd` is canonicalised and must resolve inside one of the caller's
 * own roots, and an empty `cwd` is refused outright
 * (`model_registry::workspace::resolve_chat_workspace`). The choices offered are therefore the
 * owning daemon's **own** project rows, whose `main_repo_path` is by construction one of those
 * roots — never a path the operator has to guess.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (§ ACP chat, § Known risks).
 */

import React from "react";
import { Code, ConnectError } from "@connectrpc/connect";
import { ConnectionService } from "../../../src/gen/connection_pb";
import {
  AcpService,
  type AcpAgentMessage,
  type AcpClientMessage,
} from "../../../src/gen/tddy/acp/v1/acp_pb";
import { ModelsAppPage } from "../../../src/components/models/ModelsAppPage";
import { daemonRpcIdentity, type DaemonHost } from "../../../src/lib/participantRole";
import { withSelectedDaemon } from "../../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../../support/rpc/inMemory";
import { mountWithPerDaemonLiveKitRpc } from "../../support/rpc/perDaemonLiveKitRpc";
import {
  ACP_PERMISSION_DENIED,
  acpAgentChunk,
  acpError,
  acpPromptEnd,
  acpRecordingSession,
  acpScriptedSession,
  newSessionRequest,
} from "../../support/rpc/acpSession";
import {
  aModelRegistryBackend,
  anAssistant,
  anLlmModel,
  anOllamaProvider,
  aProject,
  FIXTURE_DAEMON,
  listedProjects,
} from "../../support/rpc/modelRegistryBackend";
import {
  modelsScreenPage as page,
  type AssistantRef,
} from "../../support/pages/modelsScreenPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const FIXTURE_HOST: DaemonHost = {
  instanceId: FIXTURE_DAEMON,
  label: `${FIXTURE_DAEMON} (this daemon)`,
};

const REPO_READER: AssistantRef = { daemonInstanceId: FIXTURE_DAEMON, name: "repo-reader" };

/** The operator's two checkouts on the assistant's daemon — the only workspaces it will accept. */
const TDDY_CODER = aProject();
const SANDBOX = aProject({
  projectId: "proj-2",
  name: "sandbox",
  mainRepoPath: "/home/dev/Code/sandbox",
});

/** The agent side of `AcpService.Session`: consumes client frames, yields agent frames. */
type AcpSessionHandler = (
  requests: AsyncIterable<AcpClientMessage>,
) => AsyncIterable<AcpAgentMessage>;

/**
 * A registry holding one assistant, with an ACP session handler and the daemon's project list
 * layered onto the same backend.
 */
function aRegistryWithAssistant(options: {
  tools: string[];
  session: AcpSessionHandler;
  projects?: ReturnType<typeof aProject>[];
}) {
  return aModelRegistryBackend({
    providers: [anOllamaProvider()],
    models: [anLlmModel()],
    assistants: [anAssistant({ tools: options.tools })],
  })
    .implement(AcpService, { session: options.session })
    .onUnary(ConnectionService.method.listProjects, () =>
      listedProjects(options.projects ?? [TDDY_CODER, SANDBOX]),
    );
}

/** An agent that answers one message and ends the turn. */
const anAnsweringAgent = () =>
  acpScriptedSession(acpAgentChunk("Repo Reader here."), acpPromptEnd());

/**
 * Mount with the assistant's owning daemon present in the common room — the state a chat is opened
 * in. The chat names that participant as the one serving its stream.
 */
const mount = (backend: ReturnType<typeof aModelRegistryBackend>) =>
  mountWithRpc(
    withSelectedDaemon(<ModelsAppPage onNavigate={cy.stub()} />, [FIXTURE_HOST], [
      daemonRpcIdentity(FIXTURE_DAEMON),
    ]),
    backend,
  );

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  // Seed inside `cy.then` so it runs *after* the queued clears above; a bare synchronous
  // `setItem` executes first and is then wiped, leaving the screen with no session token.
  cy.then(() => window.localStorage.setItem("tddy_session_token", "fake-token"));
});

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("AssistantChatAcceptance — chatting with an assistant", () => {
  it("chats with a tool-less assistant without asking where its tools would run", () => {
    // Given — an assistant that reaches no tool engine at all
    mount(aRegistryWithAssistant({ tools: [], session: anAnsweringAgent() }));

    // When
    page.openAssistantChat(REPO_READER);

    // Then — straight into the conversation; nothing to confine, so nothing to ask
    page.chatDialog().should("be.visible");
    page.chatTranscript().should("contain.text", "Repo Reader here.");
    page.workspaceDialog({ timeout: 0 }).should("not.exist");
  });

  it("names the assistant, and no workspace, in a tool-less chat's handshake", () => {
    // Given
    const recorder = acpRecordingSession([acpAgentChunk("Repo Reader here.")]);
    mount(aRegistryWithAssistant({ tools: [], session: recorder.session }));

    // When
    page.openAssistantChat(REPO_READER);

    // Then — the daemon resolves the assistant's own provider, model, prompt and tools from its id;
    // re-stating them from the browser would let a stale table decide what the assistant is
    page.chatTranscript().should("contain.text", "Repo Reader here.");
    cy.wrap(recorder).should((r) => {
      expect(r.opened).to.have.length(1);
      const opened = newSessionRequest(r.opened[0]);
      expect(opened.cwd).to.equal("");
      expect(opened.modelTarget?.assistantId).to.equal("asst-1");
    });
  });

  it("does not open a chat with a tool-bearing assistant until a workspace is chosen", () => {
    // Given — an assistant that can read and grep, which the daemon will only run inside a
    // directory this operator owns
    mount(aRegistryWithAssistant({ tools: ["Read", "Grep"], session: anAnsweringAgent() }));

    // When
    page.openAssistantChat(REPO_READER);

    // Then — the question is asked first; a stream opened now would be refused mid-handshake
    page.workspaceDialog().should("be.visible");
    page.chatDialog({ timeout: 0 }).should("not.exist");
  });

  it("offers the projects the assistant's own daemon holds as its workspaces", () => {
    // Given
    mount(aRegistryWithAssistant({ tools: ["Read"], session: anAnsweringAgent() }));

    // When
    page.openAssistantChat(REPO_READER);

    // Then — the daemon resolves a chat's cwd against these very rows, so each offered path is one
    // it will accept
    page
      .offeredWorkspaces()
      .should("deep.equal", ["/home/dev/Code/tddy-coder", "/home/dev/Code/sandbox"]);
  });

  it("reads only the owning daemon's own project rows, not its peers'", () => {
    // Given
    const backend = aRegistryWithAssistant({ tools: ["Read"], session: anAnsweringAgent() });
    mount(backend);

    // When
    page.openAssistantChat(REPO_READER);
    page.workspaceDialog().should("be.visible");

    // Then — a fanned-out ListProjects also returns peers' rows, whose paths exist on other hosts
    // and would be refused by this one
    cy.wrap(backend).should((b) => {
      const calls = b.callsTo(ConnectionService.method.listProjects);
      expect(calls).to.have.length(1);
      expect(calls[0].localOnly).to.equal(true);
    });
  });

  it("opens the chat in the workspace the operator chose", () => {
    // Given
    const recorder = acpRecordingSession([acpAgentChunk("Repo Reader here.")]);
    mount(aRegistryWithAssistant({ tools: ["Read"], session: recorder.session }));

    // When
    page.openAssistantChat(REPO_READER);
    page.chooseWorkspace("proj-2");

    // Then — the handshake carries the chosen checkout, and the pane says which one it is
    page.chatDialog().should("be.visible");
    page.chatWorkspace().should("have.text", "/home/dev/Code/sandbox");
    cy.wrap(recorder).should((r) => {
      expect(r.opened).to.have.length(1);
      const opened = newSessionRequest(r.opened[0]);
      expect(opened.cwd).to.equal("/home/dev/Code/sandbox");
      expect(opened.modelTarget?.assistantId).to.equal("asst-1");
    });
  });

  it("leaves no chat open when the operator cancels the workspace choice", () => {
    // Given
    mount(aRegistryWithAssistant({ tools: ["Read"], session: anAnsweringAgent() }));

    // When
    page.openAssistantChat(REPO_READER);
    page.cancelWorkspaceChoice();

    // Then
    page.workspaceDialog({ timeout: 0 }).should("not.exist");
    page.chatDialog({ timeout: 0 }).should("not.exist");
  });

  it("says the daemon holds no project of the operator's rather than offering an empty choice", () => {
    // Given — the operator has registered nothing on this host
    mount(aRegistryWithAssistant({ tools: ["Read"], session: anAnsweringAgent(), projects: [] }));

    // When
    page.openAssistantChat(REPO_READER);

    // Then — a picker with no options and no explanation would read as a screen that had not
    // finished loading
    page.workspaceEmptyStatus().should("equal", "ready");
    page.workspaceEmptyState().should("contain.text", "Add one on the Projects screen first.");
    page.chatDialog({ timeout: 0 }).should("not.exist");
  });

  it("reports why the daemon's projects could not be read instead of offering none", () => {
    // Given — the daemon serves its registry but not its project list
    const backend = aModelRegistryBackend({
      providers: [anOllamaProvider()],
      models: [anLlmModel()],
      assistants: [anAssistant({ tools: ["Read"] })],
    })
      .implement(AcpService, { session: anAnsweringAgent() })
      .onUnary(ConnectionService.method.listProjects, () => {
        throw new ConnectError("could not resolve projects path", Code.Internal);
      });
    mount(backend);

    // When
    page.openAssistantChat(REPO_READER);

    // Then — "this host has no project" and "nobody could ask it" are different sentences
    page.workspaceError().should("contain.text", "could not resolve projects path");
    page.chatDialog({ timeout: 0 }).should("not.exist");
  });

  it("puts the daemon's refusal of a workspace into words rather than a dead stream", () => {
    // Given — the daemon judges the chosen path outside every root this operator owns, which is how
    // a symlink out of a project checkout is answered
    const refusal =
      "'/home/dev/Code/sandbox' is outside every directory this operator's sessions and projects live in";
    mount(
      aRegistryWithAssistant({
        tools: ["Read"],
        session: acpScriptedSession(acpError(refusal, ACP_PERMISSION_DENIED)),
      }),
    );

    // When
    page.openAssistantChat(REPO_READER);
    page.chooseWorkspace("proj-2");

    // Then — a chat that simply never answers is the one outcome an operator cannot act on
    page.chatError().should("have.text", refusal);
  });
});

describe("AssistantChatAcceptance — routing to the assistant's owning daemon", () => {
  /** Host A — selected first by `SelectedDaemonProvider`; owns no assistant. */
  const HOST_A: DaemonHost = { instanceId: "workstation-1", label: "workstation-1 (this daemon)" };
  /** Host B — a peer daemon, never selected, which owns the assistant being chatted with. */
  const HOST_B: DaemonHost = { instanceId: "server-2", label: "server-2" };

  const REVIEWER_ON_B: AssistantRef = { daemonInstanceId: HOST_B.instanceId, name: "reviewer" };

  it("asks the owning daemon for its workspaces and opens the chat against it", () => {
    // Given — the assistant lives on the unselected host, with a checkout only that host has
    const recorder = acpRecordingSession([acpAgentChunk("server-2 Reviewer here.")]);
    const backendA = aModelRegistryBackend({
      providers: [anOllamaProvider({ daemonInstanceId: HOST_A.instanceId })],
      models: [anLlmModel({ daemonInstanceId: HOST_A.instanceId })],
    }).onUnary(ConnectionService.method.listProjects, () =>
      listedProjects([aProject({ projectId: "proj-a", mainRepoPath: "/home/dev/on-workstation" })]),
    );
    const backendB = aModelRegistryBackend({
      providers: [anOllamaProvider({ daemonInstanceId: HOST_B.instanceId })],
      models: [],
      assistants: [
        anAssistant({
          assistantId: "asst-b1",
          name: "reviewer",
          label: "server-2 Reviewer",
          tools: ["Read"],
          daemonInstanceId: HOST_B.instanceId,
        }),
      ],
    })
      .implement(AcpService, { session: recorder.session })
      .onUnary(ConnectionService.method.listProjects, () =>
        listedProjects([aProject({ projectId: "proj-b", mainRepoPath: "/srv/checkouts/tddy-coder" })]),
      );
    mountWithPerDaemonLiveKitRpc(
      withSelectedDaemon(<ModelsAppPage onNavigate={cy.stub()} />, [HOST_A, HOST_B], [
        daemonRpcIdentity(HOST_A.instanceId),
        daemonRpcIdentity(HOST_B.instanceId),
      ]),
      {
        [daemonRpcIdentity(HOST_A.instanceId)]: backendA,
        [daemonRpcIdentity(HOST_B.instanceId)]: backendB,
      },
      { httpBackend: backendA },
    );

    // When
    page.openAssistantChat(REVIEWER_ON_B);

    // Then — host B's checkouts were offered, and host B served the stream. Host A's path would be
    // meaningless on the daemon that has to canonicalise it
    page.offeredWorkspaces().should("deep.equal", ["/srv/checkouts/tddy-coder"]);
    page.chooseWorkspace("proj-b");
    page.chatTranscript().should("contain.text", "server-2 Reviewer here.");
    cy.wrap(recorder).should((r) => {
      expect(r.opened).to.have.length(1);
      const opened = newSessionRequest(r.opened[0]);
      expect(opened.cwd).to.equal("/srv/checkouts/tddy-coder");
      expect(opened.modelTarget?.assistantId).to.equal("asst-b1");
    });
  });
});
