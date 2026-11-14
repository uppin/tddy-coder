/**
 * Unit tests for what the orchestrator composer (`#orchestrators` 8/11) asks `StartSession` for:
 * the orchestrator role and the operator's five choices go out, and nothing the daemon decides for
 * an orchestrator does — it places one on the project's home worktree itself.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-10-orchestrator-composer.md
 */

import { describe, expect, it } from "bun:test";
import {
  buildOrchestratorComposerRequest,
  orchestratorSessionTypeFor,
  type OrchestratorComposerValues,
} from "./orchestratorStartRequest";
import type { SessionAttachmentInit } from "./hooks/useSessionAttachments";
import type { SelectableAgent } from "./selectableAgentOptions";

const ORCHESTRATOR_ROLE = "orchestrator";

const A_STAGED_DOCUMENT: SessionAttachmentInit = {
  basename: "brief.md",
  source: { case: "staged", value: { daemonInstanceId: "host-1", stagingId: "staging-1", fileName: "brief.md" } },
};

/** A composer filled in on every control. */
function aComposer(overrides: Partial<OrchestratorComposerValues> = {}): OrchestratorComposerValues {
  return {
    sessionToken: "tok",
    projectId: "proj-beta",
    daemonInstanceId: "host-2",
    sessionType: "claude-cli",
    initialPrompt: "Plan the next stack",
    model: "sonnet",
    effort: "high",
    providerAccountId: "acct-meta",
    providerBuiltIn: false,
    attachments: [A_STAGED_DOCUMENT],
    ...overrides,
  };
}

/** An agent row as a host serves it; only what the session-type pick reads is set. */
function anAgent(overrides: Partial<SelectableAgent> & Pick<SelectableAgent, "id">): SelectableAgent {
  return {
    label: overrides.id,
    daemonInstanceId: "host-1",
    sessionType: `${overrides.id}-cli`,
    capabilities: [],
    runParams: [],
    permissionModes: [],
    defaultModel: "",
    placements: [],
    ...overrides,
  };
}

describe("buildOrchestratorComposerRequest", () => {
  it("the request names the orchestrator role, the project, the prompt, model, effort, account, host and attachments", () => {
    // Given
    const composer = aComposer();

    // When
    const request = buildOrchestratorComposerRequest(composer);

    // Then
    expect(request).toMatchObject({
      sessionToken: "tok",
      sessionRole: ORCHESTRATOR_ROLE,
      sessionType: "claude-cli",
      projectId: "proj-beta",
      initialPrompt: "Plan the next stack",
      model: "sonnet",
      effort: "high",
      providerAccountId: "acct-meta",
      providerBuiltIn: false,
      daemonInstanceId: "host-2",
      attachments: [A_STAGED_DOCUMENT],
    });
  });

  it("names no worktree, branch, placement or stack: the daemon places an orchestrator itself", () => {
    // Given
    const composer = aComposer();

    // When
    const request = buildOrchestratorComposerRequest(composer);

    // Then
    expect(request.sessionRole).toBe(ORCHESTRATOR_ROLE);
    for (const daemonOwned of [
      "branchWorktreeIntent",
      "newBranchName",
      "selectedIntegrationBaseRef",
      "selectedBranchToWorkOn",
      "sandbox",
      "sandboxedCodebase",
      "managedCodebase",
      "codebaseDaemonInstanceId",
      "sshConfigHost",
      "stackParent",
      "stackNodeId",
    ]) {
      expect(request, daemonOwned).not.toHaveProperty(daemonOwned);
    }
  });

  it("an untouched effort and account leave the agent's own default and its vendor", () => {
    // Given
    const composer = aComposer({ effort: "", providerAccountId: "", providerBuiltIn: false });

    // When
    const request = buildOrchestratorComposerRequest(composer);

    // Then
    expect(request).toMatchObject({
      sessionRole: ORCHESTRATOR_ROLE,
      effort: "",
      providerAccountId: "",
      providerBuiltIn: false,
    });
  });

  it("a composer with nothing attached sends no attachments", () => {
    // Given
    const composer = aComposer({ attachments: [] });

    // When
    const request = buildOrchestratorComposerRequest(composer);

    // Then
    expect(request).toMatchObject({ sessionRole: ORCHESTRATOR_ROLE, attachments: [] });
  });
});

describe("orchestratorSessionTypeFor", () => {
  it("is the first agent on the host whose spec declares project_home_main", () => {
    // Given
    const agents = [
      anAgent({ id: "cursor", capabilities: ["managed_workflow"] }),
      anAgent({ id: "claude", capabilities: ["managed_workflow", "project_home_main"] }),
      anAgent({ id: "acme", capabilities: ["project_home_main"] }),
    ];

    // When
    const sessionType = orchestratorSessionTypeFor(agents, "host-1");

    // Then
    expect(sessionType).toBe("claude-cli");
  });

  it("ignores another host's agents", () => {
    // Given
    const agents = [
      anAgent({ id: "claude", daemonInstanceId: "host-2", capabilities: ["project_home_main"] }),
      anAgent({ id: "acme", daemonInstanceId: "host-1", capabilities: ["project_home_main"] }),
    ];

    // When
    const sessionType = orchestratorSessionTypeFor(agents, "host-1");

    // Then
    expect(sessionType).toBe("acme-cli");
  });

  it("is none when the host serves no agent that can be held read-only", () => {
    // Given
    const agents = [anAgent({ id: "cursor", capabilities: ["managed_workflow"] })];

    // When
    const sessionType = orchestratorSessionTypeFor(agents, "host-1");

    // Then
    expect(sessionType).toBeNull();
  });
});
