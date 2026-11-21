/**
 * Acceptance: an orchestrator's request for access to paths beyond its reach shows in its session
 * pane with the paths, the mode and the agent's reason, and Allow or Deny answers it through
 * `AnswerPathAccess` (`#orchestra` 6/9).
 *
 * Feature: docs/ft/daemon/project-concept.md § Orchestrator sessions
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  ActivityService,
  AnswerPathAccessResponseSchema,
  ListPathAccessRequestsResponseSchema,
  PathAccessRequestSchema,
} from "tddy-client-web/gen/activity_pb";
import { PathAccessRequests } from "../../src/PathAccessRequests";
import { recordedFields } from "../support/rpc/recordedRequests";
import { byTestId } from "../support/testIds";

const ORCHESTRATOR = "01a1fdd8-0000-7000-8000-00000000000a";

const A_WRITE_REQUEST = create(PathAccessRequestSchema, {
  requestId: "access-1",
  paths: ["/srv/shared/fixtures", "/srv/shared/schemas"],
  mode: "write",
  reason: "update the shared fixtures the new session depends on",
  requestedAtUnixMs: BigInt(1_000),
});

/** Mount the request list of an orchestrator whose daemon holds `requests`. */
function mountTheRequestsOf(requests: (typeof A_WRITE_REQUEST)[]) {
  const backend = anInMemoryRpcBackend();
  backend.onUnary(ActivityService.method.listPathAccessRequests, () =>
    create(ListPathAccessRequestsResponseSchema, { requests }),
  );
  backend.onUnary(ActivityService.method.answerPathAccess, () =>
    create(AnswerPathAccessResponseSchema, {}),
  );
  cy.mountWithClients(
    <PathAccessRequests
      sessionId={ORCHESTRATOR}
      sessionToken="tok"
      client={createClient(ActivityService, backend.transport())}
    />,
    backend,
  );
  return backend;
}

describe("An orchestrator's path access requests in its session pane", () => {
  it("shows each requested path, the mode and the agent's reason", () => {
    // Given / When
    mountTheRequestsOf([A_WRITE_REQUEST]);

    // Then
    byTestId("path-access-request-access-1")
      .should("contain.text", "/srv/shared/fixtures")
      .and("contain.text", "/srv/shared/schemas")
      .and("contain.text", "write")
      .and("contain.text", "update the shared fixtures the new session depends on");
  });

  it("Allow answers the request allowed, as the orchestrator's session", () => {
    // Given
    const backend = mountTheRequestsOf([A_WRITE_REQUEST]);

    // When
    byTestId("path-access-allow-access-1").click();

    // Then
    cy.wrap(null).should(() => {
      expect(recordedFields(backend.callsTo(ActivityService.method.answerPathAccess))).to.deep.equal([
        { sessionToken: "tok", sessionId: ORCHESTRATOR, requestId: "access-1", allow: true },
      ]);
    });
  });

  it("Deny answers the request denied", () => {
    // Given
    const backend = mountTheRequestsOf([A_WRITE_REQUEST]);

    // When
    byTestId("path-access-deny-access-1").click();

    // Then
    cy.wrap(null).should(() => {
      expect(recordedFields(backend.callsTo(ActivityService.method.answerPathAccess))).to.deep.equal([
        { sessionToken: "tok", sessionId: ORCHESTRATOR, requestId: "access-1", allow: false },
      ]);
    });
  });

  it("an answered request leaves the list", () => {
    // Given
    mountTheRequestsOf([A_WRITE_REQUEST]);

    // When
    byTestId("path-access-allow-access-1").click();

    // Then
    byTestId("path-access-request-access-1").should("not.exist");
  });
});
