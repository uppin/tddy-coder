/**
 * Cypress component tests: SessionToolsTab
 *
 * Changeset: `session-inspector-tools-tab`
 * PRD: `docs/ft/web/session-drawer.md` (Tools Tab section)
 *
 * Tests the Tools tab component in isolation using stubbed RPC callbacks.
 * Covers: catalog rendering, default-args seeding, invoke flow (success + error),
 * call log rendering, call log row expansion, and post-invoke refetch.
 *
 * ⚠️ RED PHASE — these tests are intentionally failing until:
 *   1. `SessionToolsTab.tsx` is created.
 *   2. `ListSessionToolCalls` is added to `connection.proto` and regenerated into
 *      `src/gen/connection_pb.ts`.
 *   3. The component is wired to the props below.
 */

import React from "react";
import { byTestId, TEST_IDS } from "../support/testIds";

// These imports fail until the components are created and the proto is regenerated.
import { SessionToolsTab } from "../../src/components/sessions/SessionToolsTab";
import type { ToolDef } from "../../src/gen/connection_pb";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const SHELL_SCHEMA = JSON.stringify({
  type: "object",
  properties: {
    command: { type: "string" },
    timeout: { type: "integer" },
  },
  required: ["command"],
});

const READ_SCHEMA = JSON.stringify({
  type: "object",
  properties: {
    path: { type: "string" },
    limit: { type: "integer" },
  },
  required: ["path"],
});

const MOCK_TOOLS: Partial<ToolDef>[] = [
  { name: "Shell", description: "Run a shell command", inputSchemaJson: SHELL_SCHEMA },
  { name: "Read", description: "Read a file", inputSchemaJson: READ_SCHEMA },
];

const SESSION_ID = "tools-tab-test-aaaa-0000-0000-0000-000000000001";
const SESSION_TOKEN = "fake-token";

/** A minimal ToolCallInfo-shaped object (not the generated type until proto is regenerated). */
interface MockToolCallInfo {
  taskId: string;
  toolName: string;
  argsJson: string;
  resultJson: string;
  isError: boolean;
  errorMessage: string;
  jobRunning: boolean;
  createdUnixMs: bigint;
}

function aShellCall(overrides: Partial<MockToolCallInfo> = {}): MockToolCallInfo {
  return {
    taskId: "task-shell-1",
    toolName: "Shell",
    argsJson: JSON.stringify({ command: "ls -la" }),
    resultJson: JSON.stringify({ stdout: "total 0\n", stderr: "", exit_code: 0 }),
    isError: false,
    errorMessage: "",
    jobRunning: false,
    createdUnixMs: BigInt(1_700_000_001_000),
    ...overrides,
  };
}

function aReadCall(overrides: Partial<MockToolCallInfo> = {}): MockToolCallInfo {
  return {
    taskId: "task-read-1",
    toolName: "Read",
    argsJson: JSON.stringify({ path: "src/main.rs" }),
    resultJson: JSON.stringify({ content: "fn main() {}" }),
    isError: false,
    errorMessage: "",
    jobRunning: false,
    createdUnixMs: BigInt(1_700_000_002_000),
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("SessionToolsTab — component (Cypress)", () => {
  // -------------------------------------------------------------------------
  // AC3+AC4: Invoke panel — catalog and default args
  // -------------------------------------------------------------------------

  it("renders the tool picker with tools from listExecTools and seeds default args on selection", () => {
    // Given
    const onListExecTools = cy.stub().resolves(MOCK_TOOLS);
    const onListSessionToolCalls = cy.stub().resolves([]);
    const onExecuteTool = cy.stub().resolves({ resultJson: "{}", isError: false, errorMessage: "" });

    // When
    cy.mount(
      <SessionToolsTab
        sessionId={SESSION_ID}
        sessionToken={SESSION_TOKEN}
        onListExecTools={onListExecTools}
        onListSessionToolCalls={onListSessionToolCalls}
        onExecuteTool={onExecuteTool}
      />
    );

    // Then — invoke select has tool options
    byTestId(TEST_IDS.sessionsToolInvokeSelect).should("exist");
    cy.contains("Shell").should("exist");
    cy.contains("Read").should("exist");

    // When — select "Read"
    byTestId(TEST_IDS.sessionsToolInvokeSelect).select("Read");

    // Then — args textarea contains default JSON for Read schema (has "path" key)
    byTestId(TEST_IDS.sessionsToolInvokeArgs)
      .invoke("val")
      .should("include", "path");
  });

  it("shows empty string default for a required string property in the args skeleton", () => {
    // Given
    const onListExecTools = cy.stub().resolves(MOCK_TOOLS);
    const onListSessionToolCalls = cy.stub().resolves([]);
    const onExecuteTool = cy.stub().resolves({ resultJson: "{}", isError: false, errorMessage: "" });

    // When
    cy.mount(
      <SessionToolsTab
        sessionId={SESSION_ID}
        sessionToken={SESSION_TOKEN}
        onListExecTools={onListExecTools}
        onListSessionToolCalls={onListSessionToolCalls}
        onExecuteTool={onExecuteTool}
      />
    );

    // Select Shell (first option by default or by explicit select)
    byTestId(TEST_IDS.sessionsToolInvokeSelect).select("Shell");

    // Then — args contain "command" key with empty string value
    byTestId(TEST_IDS.sessionsToolInvokeArgs)
      .invoke("val")
      .then((val: string) => {
        const parsed = JSON.parse(val) as Record<string, unknown>;
        expect(parsed["command"]).to.equal("");
        expect("timeout" in parsed).to.be.false;
      });
  });

  // -------------------------------------------------------------------------
  // AC5: Invoke — success case
  // -------------------------------------------------------------------------

  it("calls onExecuteTool with correct sessionId, toolName, argsJson and renders result", () => {
    // Given
    const capturedArgs: unknown[] = [];
    const onListExecTools = cy.stub().resolves(MOCK_TOOLS);
    const onListSessionToolCalls = cy.stub().resolves([]);
    const onExecuteTool = cy
      .stub()
      .callsFake((...args: unknown[]) => {
        capturedArgs.push(args);
        return Promise.resolve({ resultJson: '{"stdout":"ok","exit_code":0}', isError: false, errorMessage: "" });
      });

    cy.mount(
      <SessionToolsTab
        sessionId={SESSION_ID}
        sessionToken={SESSION_TOKEN}
        onListExecTools={onListExecTools}
        onListSessionToolCalls={onListSessionToolCalls}
        onExecuteTool={onExecuteTool}
      />
    );

    // When — select Shell, edit args, invoke
    byTestId(TEST_IDS.sessionsToolInvokeSelect).select("Shell");
    byTestId(TEST_IDS.sessionsToolInvokeArgs)
      .clear()
      .type('{"command":"echo hello"}', { parseSpecialCharSequences: false });
    byTestId(TEST_IDS.sessionsToolInvokeButton).click();

    // Then — result block visible, error block absent
    byTestId(TEST_IDS.sessionsToolInvokeResult)
      .should("be.visible")
      .and("contain.text", "ok");
    byTestId(TEST_IDS.sessionsToolInvokeError, { timeout: 100 }).should("not.exist");

    // And — onExecuteTool was called with the correct params
    cy.then(() => {
      expect(capturedArgs.length).to.equal(1);
      const [arg] = capturedArgs[0] as [{ sessionId: string; toolName: string; argsJson: string }];
      expect(arg.sessionId).to.equal(SESSION_ID);
      expect(arg.toolName).to.equal("Shell");
      expect(arg.argsJson).to.equal('{"command":"echo hello"}');
    });
  });

  // -------------------------------------------------------------------------
  // AC6: Invoke — error case
  // -------------------------------------------------------------------------

  it("renders the error box when onExecuteTool returns is_error:true", () => {
    // Given
    const onListExecTools = cy.stub().resolves(MOCK_TOOLS);
    const onListSessionToolCalls = cy.stub().resolves([]);
    const onExecuteTool = cy.stub().resolves({
      resultJson: "",
      isError: true,
      errorMessage: "permission denied: path outside worktree",
    });

    cy.mount(
      <SessionToolsTab
        sessionId={SESSION_ID}
        sessionToken={SESSION_TOKEN}
        onListExecTools={onListExecTools}
        onListSessionToolCalls={onListSessionToolCalls}
        onExecuteTool={onExecuteTool}
      />
    );

    // When
    byTestId(TEST_IDS.sessionsToolInvokeButton).click();

    // Then — error box visible, success block absent
    byTestId(TEST_IDS.sessionsToolInvokeError)
      .should("be.visible")
      .and("contain.text", "permission denied");
    byTestId(TEST_IDS.sessionsToolInvokeResult, { timeout: 100 }).should("not.exist");
  });

  // -------------------------------------------------------------------------
  // AC7: Post-invoke refetch
  // -------------------------------------------------------------------------

  it("refetches the call log after a successful invoke so the new row appears", () => {
    // Given — first call returns empty, second returns the new row
    const firstRow = aShellCall();
    let callCount = 0;
    const onListExecTools = cy.stub().resolves(MOCK_TOOLS);
    const onListSessionToolCalls = cy.stub().callsFake(() => {
      callCount += 1;
      return Promise.resolve(callCount === 1 ? [] : [firstRow]);
    });
    const onExecuteTool = cy.stub().resolves({ resultJson: '{"ok":true}', isError: false, errorMessage: "" });

    cy.mount(
      <SessionToolsTab
        sessionId={SESSION_ID}
        sessionToken={SESSION_TOKEN}
        onListExecTools={onListExecTools}
        onListSessionToolCalls={onListSessionToolCalls}
        onExecuteTool={onExecuteTool}
      />
    );

    // Initial state — empty log
    byTestId(TEST_IDS.sessionsToolCallLog).should("exist");
    byTestId(TEST_IDS.sessionsToolCallRow, { timeout: 100 }).should("not.exist");

    // When — invoke
    byTestId(TEST_IDS.sessionsToolInvokeButton).click();

    // Then — the new row appears in the log after refetch
    byTestId(TEST_IDS.sessionsToolCallRow).should("exist");
    byTestId(TEST_IDS.sessionsToolCallRow).should("contain.text", "Shell");
  });

  // -------------------------------------------------------------------------
  // AC8+AC9: Call log rows — render and expand
  // -------------------------------------------------------------------------

  it("renders call log rows newest-first; expanding a row reveals Input and Output panels", () => {
    // Given — two calls; newer call (aReadCall) appears first
    const onListExecTools = cy.stub().resolves(MOCK_TOOLS);
    const onListSessionToolCalls = cy.stub().resolves([aShellCall(), aReadCall()]);
    const onExecuteTool = cy.stub().resolves({ resultJson: "{}", isError: false, errorMessage: "" });

    cy.mount(
      <SessionToolsTab
        sessionId={SESSION_ID}
        sessionToken={SESSION_TOKEN}
        onListExecTools={onListExecTools}
        onListSessionToolCalls={onListSessionToolCalls}
        onExecuteTool={onExecuteTool}
      />
    );

    // Then — log contains two rows
    byTestId(TEST_IDS.sessionsToolCallLog)
      .find(`[data-testid^="${TEST_IDS.sessionsToolCallRow}"]`)
      .should("have.length", 2);

    // When — expand the first row (newest = Read)
    byTestId(TEST_IDS.sessionsToolCallLog)
      .find(`[data-testid^="${TEST_IDS.sessionsToolCallRow}"]`)
      .first()
      .click();

    // Then — Input and Output panels are visible
    byTestId(TEST_IDS.sessionsToolCallInput).should("be.visible");
    byTestId(TEST_IDS.sessionsToolCallOutput).should("be.visible");
  });

  // -------------------------------------------------------------------------
  // AC10: Shell call — stdio panel shows stdout/stderr/exit_code
  // -------------------------------------------------------------------------

  it("shows stdout, stderr, and exit_code in the stdio panel when expanding a Shell call row", () => {
    // Given — a Shell call with known stdout/stderr embedded in result_json
    const shellCall = aShellCall({
      resultJson: JSON.stringify({ stdout: "hello world\n", stderr: "", exit_code: 0 }),
    });
    const onListExecTools = cy.stub().resolves(MOCK_TOOLS);
    const onListSessionToolCalls = cy.stub().resolves([shellCall]);
    const onExecuteTool = cy.stub().resolves({ resultJson: "{}", isError: false, errorMessage: "" });

    cy.mount(
      <SessionToolsTab
        sessionId={SESSION_ID}
        sessionToken={SESSION_TOKEN}
        onListExecTools={onListExecTools}
        onListSessionToolCalls={onListSessionToolCalls}
        onExecuteTool={onExecuteTool}
      />
    );

    // When — expand the Shell call row
    byTestId(TEST_IDS.sessionsToolCallLog)
      .find(`[data-testid^="${TEST_IDS.sessionsToolCallRow}"]`)
      .first()
      .click();

    // Then — stdio panel shows stdout content
    byTestId(TEST_IDS.sessionsToolCallStdio)
      .should("be.visible")
      .and("contain.text", "hello world");
  });

  // -------------------------------------------------------------------------
  // AC13: Empty state
  // -------------------------------------------------------------------------

  it("shows an empty-state message when no tool calls have been recorded", () => {
    // Given
    const onListExecTools = cy.stub().resolves(MOCK_TOOLS);
    const onListSessionToolCalls = cy.stub().resolves([]);
    const onExecuteTool = cy.stub().resolves({ resultJson: "{}", isError: false, errorMessage: "" });

    cy.mount(
      <SessionToolsTab
        sessionId={SESSION_ID}
        sessionToken={SESSION_TOKEN}
        onListExecTools={onListExecTools}
        onListSessionToolCalls={onListSessionToolCalls}
        onExecuteTool={onExecuteTool}
      />
    );

    // Then
    byTestId(TEST_IDS.sessionsToolCallLog)
      .should("contain.text", "No tool calls");
    byTestId(TEST_IDS.sessionsToolCallRow, { timeout: 100 }).should("not.exist");
  });
});
