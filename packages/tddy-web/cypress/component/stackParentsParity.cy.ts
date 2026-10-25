/**
 * `DescribeSessionStartOptions` offers the stack parents and stack bases the New-session form offers
 * (`#start-parity` 5/9).
 *
 * The form's own `prStackOrchestrators` and `stackBaseSessionCandidates` are run over the fixture
 * the tool's Rust test filters too (`tddy-tool-engine/tests/start_options.rs`), so both are held to
 * one expected answer.
 *
 * Pure-function spec: nothing is mounted.
 *
 * Changeset: docs/dev/1-WIP/2026-10-25-start-parity-start-options.md
 */

import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema, type SessionEntry } from "../../src/gen/session_pb";
import { prStackOrchestrators, stackBaseSessionCandidates } from "../../src/utils/stackParents";
import fixture from "../support/fixtures/stack-parent-candidates.json";

function asSessionEntry(row: (typeof fixture.sessions)[number]): SessionEntry {
  return create(SessionEntrySchema, {
    sessionId: row.session_id,
    projectId: row.project_id,
    daemonInstanceId: row.daemon_instance_id,
    branch: row.branch,
    recipe: row.recipe,
    orchestratorSessionId: row.orchestrator_session_id,
  });
}

const entries = () => fixture.sessions.map(asSessionEntry);
const ids = (sessions: SessionEntry[]) => sessions.map((s) => s.sessionId);

describe("DescribeSessionStartOptions parity with the New-session form's stack pickers", () => {
  it("offers the form's stack parents", () => {
    expect(ids(prStackOrchestrators(entries()))).to.deep.equal(fixture.stack_parents);
  });

  it("offers the form's stack bases for the same project and host", () => {
    const bases = stackBaseSessionCandidates(entries(), {
      projectId: fixture.scope.project_id,
      daemonInstanceId: fixture.scope.daemon_instance_id,
    });
    expect(ids(bases)).to.deep.equal(fixture.pr_stack_base_candidates);
  });
});
