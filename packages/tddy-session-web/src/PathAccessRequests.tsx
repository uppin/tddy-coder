/**
 * An orchestrator's requests for access to paths beyond its reach, in its session pane, each with
 * Allow and Deny (`#orchestra` 6/9). The daemon holds the requests (`ListPathAccessRequests`); an
 * answer is `AnswerPathAccess`, after which the request leaves the list. Allowing grants the paths
 * for the rest of the session; the agent's waiting `RequestPathAccess` returns with the answer.
 *
 * Feature doc: docs/ft/daemon/project-concept.md § Orchestrator sessions.
 */

import type { Client } from "@connectrpc/connect";
import type { ActivityService } from "tddy-client-web/gen/activity_pb";

export interface PathAccessRequestsProps {
  /** The orchestrator session whose requests are shown. */
  sessionId: string;
  sessionToken: string;
  client: Client<typeof ActivityService>;
}

/** The pending requests, oldest first; renders nothing when there are none. */
export function PathAccessRequests(props: PathAccessRequestsProps) {
  // TODO(access-elicitation): implement — list, render paths/mode/reason, answer.
  void props;
  return null;
}
