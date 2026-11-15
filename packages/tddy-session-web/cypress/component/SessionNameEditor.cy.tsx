/**
 * Acceptance: a session's name is edited in place at the top of the inspector's Details tab. The
 * label shows the name — or the derived label, muted, while unnamed — and "Rename" (or a
 * double-click on the label) turns it into a text input prefilled with the current name.
 *
 * - Enter saves the **trimmed** text; an emptied input saves "" — a clear, back to the derived label.
 * - Escape cancels, and so does blur: leaving the field is never a save.
 * - While saving the input is disabled; a refusal is shown inline and the editor stays open with the
 *   operator's text.
 *
 * The editor is pure UI: `onRename` is the whole of its contract with the host, so nothing here
 * makes an RPC.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { Code, ConnectError } from "@connectrpc/connect";
import { SessionEntrySchema, type SessionEntry } from "tddy-client-web/gen/session_pb";
import { SessionNameEditor } from "../../src/SessionNameEditor";
import {
  byTestId,
  sessionNameError,
  sessionNameInput,
  sessionNameLabel,
  sessionNameRenameBtn,
} from "../support/testIds";

const NAME = "Fix the login flow";
const REFUSAL = "display name is longer than 80 characters";

function aSession(overrides: { displayName?: string } = {}): SessionEntry {
  return create(SessionEntrySchema, {
    sessionId: "named-1",
    createdAt: "2026-11-15T10:00:00Z",
    status: "active",
    isActive: true,
    pid: 4242,
    repoPath: "/home/dev/my-feature-branch",
    ...overrides,
  });
}

const A_NAMED_SESSION = aSession({ displayName: NAME });
const AN_UNNAMED_SESSION = aSession({ displayName: "" });

/** An `onRename` that accepts every name. */
function anAcceptingRename() {
  return cy.stub().as("onRename").resolves(undefined);
}

function theEditorFor(session: SessionEntry, onRename?: (displayName: string) => Promise<void>) {
  cy.mount(<SessionNameEditor session={session} onRename={onRename} />);
}

const label = (session: SessionEntry) => byTestId(sessionNameLabel(session.sessionId));
const renameButton = (session: SessionEntry) => byTestId(sessionNameRenameBtn(session.sessionId));
const input = (session: SessionEntry) => byTestId(sessionNameInput(session.sessionId));
const error = (session: SessionEntry) => byTestId(sessionNameError(session.sessionId));

beforeEach(() => {
  cy.viewport(480, 320);
});

describe("SessionNameEditor — showing the name", () => {
  it("a named session shows its name", () => {
    // When
    theEditorFor(A_NAMED_SESSION, anAcceptingRename());

    // Then
    label(A_NAMED_SESSION).should("have.text", NAME);
    input(A_NAMED_SESSION).should("not.exist");
  });

  it("an unnamed session shows its derived label, marked unnamed", () => {
    // When
    theEditorFor(AN_UNNAMED_SESSION, anAcceptingRename());

    // Then
    label(AN_UNNAMED_SESSION)
      .should("have.text", "my-feature-branch")
      .and("have.attr", "data-unnamed", "true");
  });

  it("without onRename the name is shown with no way to edit it", () => {
    // Given
    theEditorFor(A_NAMED_SESSION);

    // When
    label(A_NAMED_SESSION).dblclick();

    // Then
    label(A_NAMED_SESSION).should("have.text", NAME);
    renameButton(A_NAMED_SESSION).should("not.exist");
    input(A_NAMED_SESSION).should("not.exist");
  });
});

describe("SessionNameEditor — entering edit mode", () => {
  it("Rename opens the editor prefilled with the current name", () => {
    // Given
    theEditorFor(A_NAMED_SESSION, anAcceptingRename());

    // When
    renameButton(A_NAMED_SESSION).click();

    // Then
    input(A_NAMED_SESSION).should("have.value", NAME).and("be.focused");
  });

  it("double-clicking the label opens the editor", () => {
    // Given
    theEditorFor(A_NAMED_SESSION, anAcceptingRename());

    // When
    label(A_NAMED_SESSION).dblclick();

    // Then
    input(A_NAMED_SESSION).should("have.value", NAME);
  });

  it("an unnamed session's editor opens empty, not on its derived label", () => {
    // Given
    theEditorFor(AN_UNNAMED_SESSION, anAcceptingRename());

    // When
    renameButton(AN_UNNAMED_SESSION).click();

    // Then
    input(AN_UNNAMED_SESSION).should("have.value", "");
  });
});

describe("SessionNameEditor — saving", () => {
  it("Enter saves the trimmed name and closes the editor", () => {
    // Given
    theEditorFor(AN_UNNAMED_SESSION, anAcceptingRename());
    renameButton(AN_UNNAMED_SESSION).click();

    // When
    input(AN_UNNAMED_SESSION).type(`  ${NAME}  {enter}`);

    // Then
    cy.get("@onRename").should("have.been.calledOnceWithExactly", NAME);
    input(AN_UNNAMED_SESSION).should("not.exist");
    label(AN_UNNAMED_SESSION).should("be.visible");
  });

  it("an emptied name saves as a clear", () => {
    // Given
    theEditorFor(A_NAMED_SESSION, anAcceptingRename());
    renameButton(A_NAMED_SESSION).click();

    // When
    input(A_NAMED_SESSION).clear().type("{enter}");

    // Then
    cy.get("@onRename").should("have.been.calledOnceWithExactly", "");
  });

  it("the input is disabled while the save is in flight", () => {
    // Given a host that has not answered yet
    const pending = cy.stub().as("onRename").returns(new Promise<void>(() => {}));
    theEditorFor(A_NAMED_SESSION, pending);
    renameButton(A_NAMED_SESSION).click();

    // When
    input(A_NAMED_SESSION).clear().type("Still saving{enter}");

    // Then
    input(A_NAMED_SESSION).should("be.disabled").and("have.value", "Still saving");
    cy.get("@onRename").should("have.been.calledOnce");
  });

  it("a refusal is shown inline and keeps the editor open with the typed text", () => {
    // Given a host that refuses the name
    const refusing = cy
      .stub()
      .as("onRename")
      .rejects(new ConnectError(REFUSAL, Code.InvalidArgument));
    theEditorFor(A_NAMED_SESSION, refusing);
    renameButton(A_NAMED_SESSION).click();

    // When
    input(A_NAMED_SESSION).clear().type("A refused name{enter}");

    // Then
    error(A_NAMED_SESSION).should("have.text", REFUSAL);
    input(A_NAMED_SESSION).should("have.value", "A refused name").and("not.be.disabled");
  });
});

describe("SessionNameEditor — cancelling", () => {
  it("Escape cancels without saving and restores the label", () => {
    // Given
    theEditorFor(A_NAMED_SESSION, anAcceptingRename());
    renameButton(A_NAMED_SESSION).click();

    // When
    input(A_NAMED_SESSION).clear().type("Abandoned{esc}");

    // Then
    input(A_NAMED_SESSION).should("not.exist");
    label(A_NAMED_SESSION).should("have.text", NAME);
    cy.get("@onRename").should("not.have.been.called");
  });

  it("leaving the field cancels without saving", () => {
    // Given
    theEditorFor(A_NAMED_SESSION, anAcceptingRename());
    renameButton(A_NAMED_SESSION).click();
    input(A_NAMED_SESSION).clear().type("Abandoned");

    // When
    input(A_NAMED_SESSION).blur();

    // Then
    input(A_NAMED_SESSION).should("not.exist");
    label(A_NAMED_SESSION).should("have.text", NAME);
    cy.get("@onRename").should("not.have.been.called");
  });
});
