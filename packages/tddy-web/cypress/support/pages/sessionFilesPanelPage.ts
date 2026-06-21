import { byTestId, TEST_IDS, sessionMoreActions } from "../testIds";

export const sessionFilesPanelPage = {
  preview: () => byTestId(TEST_IDS.sessionFilePreview),
  yamlHighlight: () => byTestId(TEST_IDS.yamlSyntaxHighlight),
};

export const sessionMoreActionsPage = {
  trigger: (sessionId: string) => byTestId(sessionMoreActions(sessionId)),
  showFiles: () => byTestId(TEST_IDS.sessionMoreActionsShowFiles),
};
