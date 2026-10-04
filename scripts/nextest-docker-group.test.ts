import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import {
  binariesStartingTheTestkit,
  dockerGroup,
  isSerialised,
  type TestBinary,
} from "./nextest-docker-group";

const REPO_ROOT = join(import.meta.dir, "..");
const NEXTEST_CONFIG = readFileSync(join(REPO_ROOT, ".config", "nextest.toml"), "utf8");

function label(binary: TestBinary) {
  return `${binary.package}::${binary.binary}`;
}

describe("the serial docker group", () => {
  test("no_binary_that_starts_the_livekit_testkit_is_in_the_serial_docker_group", () => {
    const group = dockerGroup(NEXTEST_CONFIG);

    const stillSerialised = binariesStartingTheTestkit(REPO_ROOT)
      .filter((binary) => isSerialised(group, binary))
      .map(label);

    expect(stillSerialised).toEqual([]);
  });

  test("every_binary_the_serial_docker_group_names_exists_in_the_package_it_names", () => {
    const group = dockerGroup(NEXTEST_CONFIG);
    const existing = new Set(
      binariesStartingTheTestkit(REPO_ROOT).map((binary) => label(binary)),
    );

    const namedButMissing = group.binaries.map(label).filter((name) => !existing.has(name));

    expect(namedButMissing).toEqual([]);
  });
});
