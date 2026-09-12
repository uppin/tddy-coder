/**
 * In-memory `session.SessionService` fake — session lifecycle RPCs for acceptance tests.
 *
 * Split out of the retired monolithic `connectionServiceBackend.ts` by `#unbundle` node 9.
 * Composed with `projectServiceBackend.ts` and the other per-service fakes by
 * `daemonSessionHostBackend.ts`.
 */

export {
  aSessionServiceBackend,
  type SessionServiceScenario,
  type SessionServiceBackend,
} from "./daemonSessionHostBackend";
