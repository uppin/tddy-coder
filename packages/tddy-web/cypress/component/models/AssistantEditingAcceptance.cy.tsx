/**
 * Acceptance: an assistant already defined on a daemon can be edited in place and deleted, and a
 * write the daemon refuses is reported against that assistant's row.
 *
 * `UpdateAssistant` carries the whole tool set, so an edit that submits the wrong set silently
 * re-arms or disarms an agent — the tools it is sent are asserted exactly.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (AC8, AC9).
 */

import React from "react";
import { Code } from "@connectrpc/connect";
import { ModelRegistryService } from "../../../src/gen/models_pb";
import { ModelsAppPage } from "../../../src/components/models/ModelsAppPage";
import type { DaemonHost } from "../../../src/lib/participantRole";
import { withSelectedDaemon } from "../../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../../support/rpc/inMemory";
import {
  aModelRegistryBackend,
  anAssistant,
  anLlmModel,
  anOllamaProvider,
  FIXTURE_DAEMON,
} from "../../support/rpc/modelRegistryBackend";
import {
  modelsScreenPage as page,
  type AssistantRef,
} from "../../support/pages/modelsScreenPage";
import { recordedFields } from "../../support/rpc/recordedRequests";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const FIXTURE_HOST: DaemonHost = {
  instanceId: FIXTURE_DAEMON,
  label: `${FIXTURE_DAEMON} (this daemon)`,
};

const REPO_READER: AssistantRef = { daemonInstanceId: FIXTURE_DAEMON, name: "repo-reader" };
/** A second assistant, so an emptied panel cannot be what makes a deletion look successful. */
const RELEASE_NOTER: AssistantRef = { daemonInstanceId: FIXTURE_DAEMON, name: "release-noter" };

const mount = (backend: ReturnType<typeof aModelRegistryBackend>) =>
  mountWithRpc(
    withSelectedDaemon(<ModelsAppPage onNavigate={cy.stub()} />, [FIXTURE_HOST]),
    backend,
  );

const aRegistryWithTwoAssistants = () =>
  aModelRegistryBackend({
    providers: [anOllamaProvider()],
    models: [anLlmModel()],
    assistants: [
      anAssistant(),
      anAssistant({
        assistantId: "asst-2",
        name: RELEASE_NOTER.name,
        label: "Release Noter",
        tools: ["Read"],
      }),
    ],
  });

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

describe("AssistantEditingAcceptance — editing and deleting a defined assistant", () => {
  it("opens an assistant's edit dialog on the values the daemon holds", () => {
    // Given
    mount(aRegistryWithTwoAssistants());

    // When
    page.openEditAssistant(REPO_READER);

    // Then — the dialog edits what is stored, rather than starting from blank fields that would
    // clear the assistant's prompt and disarm it on the first save
    page.editAssistantLabelField().should("have.value", "Repo Reader");
    page
      .editAssistantSystemPromptField()
      .should("have.value", "You read code and answer questions about it.");
  });

  it("saves an edited system prompt and tool set to the owning daemon", () => {
    // Given — an assistant that reads code, to be widened into one that may also write
    const backend = aRegistryWithTwoAssistants();
    mount(backend);

    // When
    page.openEditAssistant(REPO_READER);
    page.fillAndSubmitEditAssistantForm({
      label: "Repo Editor",
      systemPrompt: "You read code and make the edits you are asked for.",
      tools: ["Read", "Write", "Grep"],
    });

    // Then — the whole tool set is carried, in the daemon's catalog order, and the row re-reads
    cy.wrap(backend).should((b) => {
      expect(recordedFields(b.callsTo(ModelRegistryService.method.updateAssistant))).to.deep.equal([
        {
          sessionToken: "fake-token",
          assistantId: "asst-1",
          label: "Repo Editor",
          systemPrompt: "You read code and make the edits you are asked for.",
          tools: ["Read", "Write", "Grep"],
        },
      ]);
    });
    page.assistantRow(REPO_READER).should("contain.text", "Repo Editor");
    page.assistantTools(REPO_READER).should("deep.equal", ["Read", "Write", "Grep"]);
  });

  it("deletes an assistant, leaving the fleet's other assistants listed", () => {
    // Given
    const backend = aRegistryWithTwoAssistants();
    mount(backend);

    // When
    page.deleteAssistant(REPO_READER);

    // Then
    cy.wrap(backend).should((b) => {
      expect(recordedFields(b.callsTo(ModelRegistryService.method.deleteAssistant))).to.deep.equal([
        { sessionToken: "fake-token", assistantId: "asst-1" },
      ]);
    });
    page.assistantRow(RELEASE_NOTER).should("exist");
    page.assistantRow(REPO_READER, { timeout: 0 }).should("not.exist");
  });

  it("reports a deletion the daemon refused as permission denied, keeping the assistant listed", () => {
    // Given — a daemon that serves writes only for the host that owns the row
    const backend = aRegistryWithTwoAssistants().failWith(
      ModelRegistryService.method.deleteAssistant,
      Code.PermissionDenied,
      "assistant asst-1 is owned by workstation-1",
    );
    mount(backend);

    // When
    page.deleteAssistant(REPO_READER);

    // Then
    page
      .assistantError(REPO_READER)
      .should("have.text", "Permission denied — assistant asst-1 is owned by workstation-1");
    page.assistantRow(REPO_READER).should("exist");
  });

  it("reports an edit the daemon rejected without closing the dialog", () => {
    // Given — a daemon that will not accept the change
    const backend = aRegistryWithTwoAssistants().failWith(
      ModelRegistryService.method.updateAssistant,
      Code.InvalidArgument,
      "tool 'Write' is not in this daemon's exec catalog",
    );
    mount(backend);

    // When
    page.openEditAssistant(REPO_READER);
    page.fillAndSubmitEditAssistantForm({
      label: "Repo Editor",
      systemPrompt: "You read code and make the edits you are asked for.",
      tools: ["Read", "Write"],
    });

    // Then — the operator keeps the edit they made, alongside the daemon's reason for refusing it
    page
      .editAssistantDialog()
      .should("contain.text", "tool 'Write' is not in this daemon's exec catalog");
    page.assistantRow(REPO_READER).should("contain.text", "Repo Reader");
  });
});
