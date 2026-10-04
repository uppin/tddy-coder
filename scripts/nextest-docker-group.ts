/**
 * Which test binaries start the LiveKit testkit, and which of them the nextest `docker` group
 * still serialises.
 *
 * The group exists because every test used to launch its own container on ports found by
 * `bind(:0)` and release. With one shared server (`LIVEKIT_TESTKIT_WS_URL`) and a unique room per
 * test, that race is gone and the group has nothing left to protect. Both sets are derived from
 * the source rather than a list a person maintains, so the config cannot drift from the tests again.
 */

export interface TestBinary {
  /** Cargo package that owns the test target. */
  package: string;
  /** The test target's name: the file name under `tests/`, without `.rs`. */
  binary: string;
}

export interface DockerGroup {
  /** Packages named wholesale: every test target they own is in the group. */
  packages: Set<string>;
  /** `package` + `binary` pairs named individually. */
  binaries: TestBinary[];
}

/** Every integration-test binary under `packages/<pkg>/tests/` whose source starts the testkit. */
export function binariesStartingTheTestkit(repoRoot: string): TestBinary[] {
  // TODO(parallel-livekit): implement
  void repoRoot;
  throw new Error("binariesStartingTheTestkit is not implemented");
}

/** What the `docker` override of the `ci` profile in `.config/nextest.toml` selects. */
export function dockerGroup(nextestToml: string): DockerGroup {
  // TODO(parallel-livekit): implement
  void nextestToml;
  throw new Error("dockerGroup is not implemented");
}

/** Whether nextest would serialise `binary` under `group`. */
export function isSerialised(group: DockerGroup, binary: TestBinary): boolean {
  // TODO(parallel-livekit): implement
  void group;
  void binary;
  throw new Error("isSerialised is not implemented");
}
