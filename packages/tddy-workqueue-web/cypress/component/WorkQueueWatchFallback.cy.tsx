/**
 * The watch fallback registered in `cypress/support/component.ts`: a backend that stubs only the unary
 * `getWorkQueue` answers `watchWorkQueue` with that stub's queue as its first snapshot, then keeps the
 * watch open — as the daemon's does until something changes — until the client cancels it. The stub is
 * asked with a `GetWorkQueueRequest` naming the watched queue as its `stack`.
 *
 * Drives the testkit directly (backend + typed client), without mounting a component.
 */

import { createClient } from "@connectrpc/connect";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { WorkQueueService } from "tddy-client-web/gen/work_queue_pb";
import { aPlannedNode, aWorkQueueJson } from "../support/rpc/prStackFixtures";

const QUEUE_JSON = aWorkQueueJson(1, [aPlannedNode({ itemId: "n1", title: "Add token store" })], 5);
const ADDRESS = { projectId: "proj-fallback", queueId: "queue-fallback", daemonInstanceId: "host-a" };

/** What a watcher saw: its first snapshot, whether the watch was still open a moment later, and how it ended once cancelled. */
interface WatchOutcome {
  readonly first: { revision: number; workQueueJson: string } | undefined;
  readonly openAfterFirst: boolean;
  readonly endedOnCancel: boolean;
  /** The queue the unary stub was asked for, and with which token. */
  readonly stubAskedFor: { queueId: string | undefined; sessionToken: string } | undefined;
}

async function watchOnce(): Promise<WatchOutcome> {
  let stubAskedFor: WatchOutcome["stubAskedFor"];
  const backend = anInMemoryRpcBackend().onUnary(WorkQueueService.method.getWorkQueue, (req) => {
    stubAskedFor = { queueId: req.stack?.queueId, sessionToken: req.sessionToken };
    return create(WorkQueueService.method.getWorkQueue.output, { workQueueJson: QUEUE_JSON });
  });
  const client = createClient(WorkQueueService, backend.transport());
  const abort = new AbortController();
  const iterator = client.watchWorkQueue({ sessionToken: "t", queue: ADDRESS }, { signal: abort.signal })[Symbol.asyncIterator]();
  const firstStep = await iterator.next();
  const first = firstStep.done
    ? undefined
    : { revision: Number(firstStep.value.revision), workQueueJson: firstStep.value.workQueueJson };
  const pending = iterator.next().then(
    () => "ended" as const,
    () => "ended" as const,
  );
  const openAfterFirst =
    (await Promise.race([pending, new Promise<"open">((resolve) => setTimeout(() => resolve("open"), 300))])) === "open";
  abort.abort();
  const endedOnCancel = (await Promise.race([pending, new Promise<"open">((resolve) => setTimeout(() => resolve("open"), 2000))])) === "ended";
  return { first, openAfterFirst, endedOnCancel, stubAskedFor };
}

it("a getWorkQueue stub, asked for the watched queue, answers the watch, which then stays open until it is cancelled", () => {
  cy.wrap(watchOnce(), { timeout: 5000 }).should("deep.equal", {
    first: { revision: 5, workQueueJson: QUEUE_JSON },
    openAfterFirst: true,
    endedOnCancel: true,
    stubAskedFor: { queueId: ADDRESS.queueId, sessionToken: "t" },
  });
});
