/**
 * Project-registry fixtures for acceptance tests (`project.ProjectService`).
 *
 * Handler wiring lives in `daemonSessionHostBackend.ts`; this module is the import surface for
 * specs that only need project collision fixtures.
 */

export { COLLISION_PROJECT_ID, daemonSessionHostProjectIdCollisionScenario } from "./daemonSessionHostBackend";
