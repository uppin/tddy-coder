import React from "react";
import { SessionFilesPanel } from "../../src/components/session/SessionFilesPanel";
import { SessionMoreActionsMenu } from "../../src/components/session/SessionMoreActionsMenu";
import { byTestId, TEST_IDS } from "../support/testIds";

const MD_FIXTURE = `# Unique Acceptance Heading

- list item one
<script>alert("xss")</script>
`;

const YAML_FIXTURE = `root_key: acceptance-yaml-7f3a
nested:
  child: value
`;

function SessionFilesPanelHarness() {
  const fileContents: Record<string, string> = {
    "changeset.yaml": YAML_FIXTURE,
    "PRD.md": MD_FIXTURE,
  };
  return (
    <SessionFilesPanel
      files={[{ basename: "changeset.yaml" }, { basename: "PRD.md" }]}
      fileContents={fileContents}
      initialSelection="PRD.md"
    />
  );
}

describe("SessionFilesPanel", () => {
  it("renders sanitized Markdown preview with heading when a .md file is selected", () => {
    // Given
    cy.mount(<SessionFilesPanelHarness />);

    // When
    cy.contains("button", "PRD.md").click();

    // Then — heading text is visible and script tag is stripped
    byTestId(TEST_IDS.sessionFilePreview).find("h1").should("contain.text", "Unique Acceptance Heading");
    byTestId(TEST_IDS.sessionFilePreview).find("script").should("not.exist");
  });

  it("renders YAML with syntax highlighting when a .yaml file is selected", () => {
    // Given
    cy.mount(<SessionFilesPanelHarness />);

    // When
    cy.contains("button", "changeset.yaml").click();

    // Then
    byTestId(TEST_IDS.yamlSyntaxHighlight).should("exist");
    cy.contains("acceptance-yaml-7f3a").should("be.visible");
  });
});

describe("SessionMoreActionsMenu", () => {
  it("overflow menu includes a Show files action with a stable test-id", () => {
    // Given
    cy.mount(<SessionMoreActionsMenu sessionId="acceptance-sess-1" onShowFiles={() => {}} />);

    // When
    byTestId("session-more-actions-acceptance-sess-1").click();

    // Then
    byTestId(TEST_IDS.sessionMoreActionsShowFiles).should("be.visible");
    cy.contains("Show files").should("be.visible");
  });
});
