import { describe, expect, it } from "bun:test";
import { aSessionEntry } from "../test-utils";
import { prStackOrchestrators } from "./stackParents";

/**
 * Tests for `prStackOrchestrators` — the recipe-based filter that identifies which sessions
 * in a list are PR-stack orchestrators eligible to be selected as a parent in the new-session
 * pane. A session is eligible when its `recipe` is one of the PR-stack kinds AND it is not
 * itself a child of another orchestrator.
 */
describe("prStackOrchestrators", () => {
  it("returns empty array when no sessions have a PR-stack recipe", () => {
    // Given — plain sessions with recipes that are not PR-stack orchestrators
    const sessions = [
      aSessionEntry({ sessionId: "tdd-1", recipe: "tdd" }),
      aSessionEntry({ sessionId: "bugfix-1", recipe: "bugfix" }),
      aSessionEntry({ sessionId: "no-recipe", recipe: "" }),
    ];

    // When
    const orchestrators = prStackOrchestrators(sessions);

    // Then — none of these sessions qualify as PR-stack orchestrators
    expect(orchestrators).toEqual([]);
  });

  it("returns sessions with orchestrate-pr-stack recipe", () => {
    // Given — one orchestrate-pr-stack session alongside a plain tdd session
    const orchestrator = aSessionEntry({ sessionId: "orch-1", recipe: "orchestrate-pr-stack" });
    const plain = aSessionEntry({ sessionId: "plain-1", recipe: "tdd" });
    const sessions = [orchestrator, plain];

    // When
    const orchestrators = prStackOrchestrators(sessions);

    // Then — only the orchestrator is returned; the plain session is excluded
    expect(orchestrators).toHaveLength(1);
    expect(orchestrators[0]!.sessionId).toBe("orch-1");
  });

  it("returns sessions with plan-pr-stack recipe", () => {
    // Given — a plan-pr-stack session (the planning variant of PR-stack orchestration)
    const planner = aSessionEntry({ sessionId: "plan-1", recipe: "plan-pr-stack" });
    const sessions = [planner];

    // When
    const orchestrators = prStackOrchestrators(sessions);

    // Then — plan-pr-stack is a valid orchestrator recipe
    expect(orchestrators).toHaveLength(1);
    expect(orchestrators[0]!.sessionId).toBe("plan-1");
  });

  it("returns both orchestrate-pr-stack and plan-pr-stack orchestrators from a mixed list", () => {
    // Given — two PR-stack orchestrators alongside two plain sessions
    const orch = aSessionEntry({ sessionId: "orch-1", recipe: "orchestrate-pr-stack" });
    const plan = aSessionEntry({ sessionId: "plan-1", recipe: "plan-pr-stack" });
    const tdd = aSessionEntry({ sessionId: "tdd-1", recipe: "tdd" });
    const noRecipe = aSessionEntry({ sessionId: "no-recipe-1", recipe: "" });
    const sessions = [orch, plan, tdd, noRecipe];

    // When
    const orchestrators = prStackOrchestrators(sessions);

    // Then — exactly the two PR-stack recipe sessions are returned
    expect(orchestrators).toHaveLength(2);
    const ids = orchestrators.map((s) => s.sessionId).sort();
    expect(ids).toEqual(["orch-1", "plan-1"]);
  });

  it("excludes PR-stack sessions that are themselves children of another orchestrator", () => {
    // Given — an orchestrate-pr-stack session that already belongs to a parent stack
    const nestedOrch = aSessionEntry({
      sessionId: "nested-orch",
      recipe: "orchestrate-pr-stack",
      orchestratorSessionId: "parent-orch",
    });
    const sessions = [nestedOrch];

    // When
    const orchestrators = prStackOrchestrators(sessions);

    // Then — a session that is already a child cannot be selected as a parent
    expect(orchestrators).toEqual([]);
  });

  it("returns empty array for an empty session list", () => {
    expect(prStackOrchestrators([])).toEqual([]);
  });
});
