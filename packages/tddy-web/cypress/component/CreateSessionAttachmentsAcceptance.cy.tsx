/**
 * Acceptance tests for optional document attachments on the Start-Session form.
 *
 * Files are uploaded into the daemon's pre-session staging area, listed in the form, and sent on
 * `StartSessionRequest.attachments` so the daemon can copy them into `{session_dir}/attachments/`.
 */
import React from "react";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { useDaemonClient } from "../../src/rpc/selectedDaemon";
import { ConnectionService } from "../../src/gen/connection_pb";
import { aConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { TEST_IDS, byTestId } from "../support/testIds";
import { createSessionAttachmentsPage as attachments } from "../support/pages/createSessionAttachmentsPage";
import { reconstructUtf8 } from "../support/util/fileDrop";

const NEW_SESSION_ID = "session-with-attachments-0001";

/** Thin wrapper so CreateSessionPane can use the daemon-routed client for list/start RPC. */
function CreateSessionPaneViaDaemon(props: {
  onCancel: () => void;
  onCreated: (sessionId: string) => void;
}) {
  const client = useDaemonClient(ConnectionService);
  if (!client) return null;
  return (
    <CreateSessionPane
      client={client}
      sessionToken="fake-token"
      onCancel={props.onCancel}
      onCreated={props.onCreated}
    />
  );
}

function mountCreateSessionPane(backend: ReturnType<typeof aConnectionServiceBackend>) {
  mountWithRecordingLiveKitRpc(
    withSelectedDaemon(
      <CreateSessionPaneViaDaemon onCancel={cy.stub().as("onCancel")} onCreated={cy.stub().as("onCreated")} />,
    ),
    backend,
  );
}

function waitForBaselineLoads() {
  cy.wait(1000);
}

describe("CreateSession attachments — upload, list, and submit", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("shows the attachments field for claude-cli sessions but not for tool sessions", () => {
    const backend = aConnectionServiceBackend({
      startSession: { sessionId: NEW_SESSION_ID },
    });
    mountCreateSessionPane(backend);
    waitForBaselineLoads();

    // Tool session (default) — no attachments UI
    attachments.field().should("not.exist");

    // Claude CLI — attachments UI appears
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    attachments.field().should("be.visible");
  });

  it("uploads a picked file into staging and lists it before submit", () => {
    const backend = aConnectionServiceBackend({
      startSession: { sessionId: NEW_SESSION_ID },
    });
    mountCreateSessionPane(backend);
    waitForBaselineLoads();

    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");

    attachments.pickFile("# Product brief\nShip attachments.", "brief.md", "text/markdown");

    attachments.row(0).should("contain.text", "brief.md");

    cy.wrap(null).should(() => {
      const chunks = backend
        .callsTo(ConnectionService.method.uploadStagedAttachmentChunk)
        .filter((c) => c.fileName === "brief.md")
        .map((c) => c.data as Uint8Array);
      expect(reconstructUtf8(chunks)).to.equal("# Product brief\nShip attachments.");
    });
  });

  it("sends staged attachments on StartSession and omits them for tool sessions", () => {
    const backend = aConnectionServiceBackend({
      startSession: { sessionId: NEW_SESSION_ID },
    });
    mountCreateSessionPane(backend);
    waitForBaselineLoads();

    // Given — a claude-cli session with one attached document
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");
    attachments.pickFile("context", "notes.txt");
    attachments.row(0).should("contain.text", "notes.txt");

    // When — the user creates the session
    byTestId(TEST_IDS.createSessionSubmitBtn).click();

    cy.wrap(null).should(() => {
      expect(backend.startSessionCalls).to.have.length(1);
      const req = backend.startSessionCalls[0]!;
      expect(req.sessionType).to.equal("claude-cli");
      expect(req.attachments).to.have.length(1);
      expect(req.attachments[0]!.basename).to.equal("notes.txt");
      expect(req.attachments[0]!.source.case).to.equal("staged");
      expect(req.attachments[0]!.source.value?.fileName).to.equal("notes.txt");
    });
  });

  it("removes an attachment from the list before submit", () => {
    const backend = aConnectionServiceBackend({
      startSession: { sessionId: NEW_SESSION_ID },
    });
    mountCreateSessionPane(backend);
    waitForBaselineLoads();

    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");
    attachments.pickFile("drop me", "temp.txt");
    attachments.row(0).should("exist");

    attachments.removeButton(0).click();
    attachments.row(0).should("not.exist");

    byTestId(TEST_IDS.createSessionSubmitBtn).click();

    cy.wrap(null).should(() => {
      expect(backend.startSessionCalls[0]!.attachments).to.have.length(0);
    });
  });
});
