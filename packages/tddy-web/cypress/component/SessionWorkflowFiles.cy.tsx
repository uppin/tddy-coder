import React from "react";
import { SessionFilesPanel } from "../../src/components/session/SessionFilesPanel";
import { SessionMoreActionsMenu } from "../../src/components/session/SessionMoreActionsMenu";

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

/** Acceptance: Markdown preview is rendered (sanitized) with visible heading text, not raw source only. */
describe("SessionFilesPanel", () => {
  it("SessionFilesPanel_renders_markdown_preview_when_md_selected", () => {
    cy.mount(<SessionFilesPanelHarness />);
    cy.contains("button", "PRD.md").click();
    cy.get('[data-testid="session-file-preview"]')
      .find("h1")
      .should("contain.text", "Unique Acceptance Heading");
    cy.get('[data-testid="session-file-preview"]').find("script").should("not.exist");
  });

  /** Acceptance: YAML preview uses structured / syntax-highlighted presentation with distinctive keys visible. */
  it("SessionFilesPanel_renders_yaml_preview_when_yaml_selected", () => {
    cy.mount(<SessionFilesPanelHarness />);
    cy.contains("button", "changeset.yaml").click();
    cy.get('[data-testid="yaml-syntax-highlight"]').should("exist");
    cy.contains("acceptance-yaml-7f3a").should("be.visible");
  });
});

describe("SessionMoreActionsMenu", () => {
  /** Acceptance: overflow exposes Show files with stable test id for automation. */
  it("MoreActionsMenu_includes_show_files", () => {
    cy.mount(<SessionMoreActionsMenu sessionId="acceptance-sess-1" onShowFiles={() => {}} />);
    cy.get('[data-testid="session-more-actions-acceptance-sess-1"]').click();
    cy.get('[data-testid="session-more-actions-show-files"]').should("be.visible");
    cy.contains("Show files").should("be.visible");
  });
});
