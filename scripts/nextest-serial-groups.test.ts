import { afterEach, describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import {
  binariesStartingTheTestkit,
  ciSerialisation,
  isSerialised,
  label,
  referencesMatchingNothing,
  serialGroupReferences,
  testBinaries,
  type TestBinary,
  workspacePackages,
} from "./nextest-serial-groups";

const REPO_ROOT = join(import.meta.dir, "..");
const NEXTEST_CONFIG = readFileSync(join(REPO_ROOT, ".config", "nextest.toml"), "utf8");

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

const SERIAL_LIVEKIT_GROUP = `
[test-groups]
livekit = { max-threads = 1 }
`;

const ROOM_TEST: TestBinary = { package: "tddy-livekit", binary: "room_roster_livekit" };

/** A nextest config whose one override, on `profile`, puts `filter` in `group`. */
function aConfigPutting(filter: string, options: { inGroup: string; onProfile: "ci" | "default" }): string {
  return `${SERIAL_LIVEKIT_GROUP}
[[profile.${options.onProfile}.overrides]]
filter = "${filter}"
test-group = "${options.inGroup}"
`;
}

function isSerialisedUnder(nextestToml: string, binary: TestBinary): boolean {
  return isSerialised(ciSerialisation(nextestToml), binary);
}

/** A cargo workspace in a temporary directory, built package by package. */
class AWorkspace {
  readonly root = mkdtempSync(join(tmpdir(), "nextest-serial-groups-"));
  private readonly members: string[] = [];

  constructor() {
    this.writeRootManifest();
  }

  /** A member package at `packages/<name>` whose files (relative to the package) are `files`. */
  withPackage(name: string, files: Record<string, string>, manifestExtra = ""): this {
    this.write(join("packages", name, "Cargo.toml"), `[package]\nname = "${name}"\nversion = "0.1.0"\n${manifestExtra}`);
    for (const [path, contents] of Object.entries(files)) this.write(join("packages", name, path), contents);
    this.members.push(`packages/${name}`);
    this.writeRootManifest();
    return this;
  }

  /** A package directory the workspace does not list as a member. */
  withNonMember(name: string, files: Record<string, string>): this {
    this.write(join("packages", name, "Cargo.toml"), `[package]\nname = "${name}"\nversion = "0.1.0"\n`);
    for (const [path, contents] of Object.entries(files)) this.write(join("packages", name, path), contents);
    return this;
  }

  remove(): void {
    rmSync(this.root, { recursive: true, force: true });
  }

  private writeRootManifest(): void {
    this.write("Cargo.toml", `[workspace]\nmembers = ${JSON.stringify(this.members)}\n`);
  }

  private write(path: string, contents: string): void {
    const full = join(this.root, path);
    mkdirSync(dirname(full), { recursive: true });
    writeFileSync(full, contents);
  }
}

const workspaces: AWorkspace[] = [];

function aWorkspace(): AWorkspace {
  const workspace = new AWorkspace();
  workspaces.push(workspace);
  return workspace;
}

afterEach(() => {
  for (const workspace of workspaces.splice(0)) workspace.remove();
});

const STARTS_THE_TESTKIT = "fn t() { let _ = LiveKitTestkit::start(); }\n";

function binaryNames(binaries: TestBinary[]): string[] {
  return binaries.map(label);
}

// ---------------------------------------------------------------------------------------------
// The repository itself: the drift check
// ---------------------------------------------------------------------------------------------

describe("the repository's serial test-groups", () => {
  test("no_binary_that_starts_the_livekit_testkit_is_in_a_serial_test_group", () => {
    const config = ciSerialisation(NEXTEST_CONFIG);

    const stillSerialised = binariesStartingTheTestkit(REPO_ROOT).filter((binary) => isSerialised(config, binary));

    expect(binaryNames(stillSerialised)).toEqual([]);
  });

  test("every_binary_a_serial_group_names_exists_in_the_package_it_names", () => {
    const references = serialGroupReferences(ciSerialisation(NEXTEST_CONFIG));
    const workspace = {
      packages: workspacePackages(REPO_ROOT).map((pkg) => pkg.name),
      binaries: testBinaries(REPO_ROOT),
    };

    const namedButMissing = referencesMatchingNothing(references, workspace);

    expect(namedButMissing).toEqual([]);
  });
});

// ---------------------------------------------------------------------------------------------
// Reading .config/nextest.toml
// ---------------------------------------------------------------------------------------------

describe("which binaries the ci profile serialises", () => {
  test("a_default_profile_override_serialises_the_ci_run", () => {
    const config = aConfigPutting("package(tddy-livekit)", { inGroup: "livekit", onProfile: "default" });

    const serialised = isSerialisedUnder(config, ROOM_TEST);

    expect(serialised).toBe(true);
  });

  test("a_negated_binary_is_not_serialised", () => {
    const config = aConfigPutting("package(tddy-livekit) and not binary(room_roster_livekit)", {
      inGroup: "livekit",
      onProfile: "ci",
    });

    const serialised = isSerialisedUnder(config, ROOM_TEST);

    expect(serialised).toBe(false);
  });

  test("a_group_without_max_threads_one_is_not_serial", () => {
    const config = `
[test-groups]
livekit = { max-threads = 2 }

[[profile.ci.overrides]]
filter = "package(tddy-livekit)"
test-group = "livekit"
`;

    const serialised = isSerialisedUnder(config, ROOM_TEST);

    expect(serialised).toBe(false);
  });

  test("the_first_override_that_selects_a_binary_decides_its_group", () => {
    const config = `${aConfigPutting("binary(room_roster_livekit)", { inGroup: "@global", onProfile: "ci" })}
[[profile.default.overrides]]
filter = "package(tddy-livekit)"
test-group = "livekit"
`;

    const serialised = isSerialisedUnder(config, ROOM_TEST);

    expect(serialised).toBe(false);
  });

  test("a_ci_profile_that_inherits_from_another_profile_is_rejected", () => {
    const config = `${aConfigPutting("package(tddy-livekit)", { inGroup: "livekit", onProfile: "ci" })}
[profile.ci]
inherits = "slow"
`;

    expect(() => ciSerialisation(config)).toThrow(/inherits/);
  });

  test("an_unrecognised_filter_token_is_rejected", () => {
    const config = aConfigPutting("package(tddy-livekit) and !binary(room_roster_livekit)", {
      inGroup: "livekit",
      onProfile: "ci",
    });

    expect(() => ciSerialisation(config)).toThrow(/unrecognised filterset token: character "!" at offset 26/);
  });

  test("a_glob_filter_argument_is_rejected", () => {
    const config = aConfigPutting("binary({room_roster_livekit,rpc_scenarios})", { inGroup: "livekit", onProfile: "ci" });

    expect(() => ciSerialisation(config)).toThrow(/only exact names are supported in a filterset, not glob syntax/);
  });

  test("a_hash_glob_filter_argument_is_rejected", () => {
    const config = aConfigPutting("binary(#room_*)", { inGroup: "livekit", onProfile: "ci" });

    expect(() => ciSerialisation(config)).toThrow(/not the # matcher/);
  });

  test("a_filter_predicate_that_does_not_select_whole_binaries_is_rejected", () => {
    const config = aConfigPutting("test(=joins_a_room)", { inGroup: "livekit", onProfile: "ci" });

    expect(() => ciSerialisation(config)).toThrow(/unsupported filterset predicate: test\(=joins_a_room\)/);
  });
});

describe("what a serial group's filter names", () => {
  const workspace = {
    packages: ["tddy-daemon", "tddy-daemon-livekit"],
    binaries: [{ package: "tddy-daemon-livekit", binary: "session_room_livekit_acceptance" }],
  };

  test("a_binary_qualified_by_a_package_that_lacks_it_matches_nothing", () => {
    const config = ciSerialisation(
      aConfigPutting("package(tddy-daemon) and binary(session_room_livekit_acceptance)", {
        inGroup: "livekit",
        onProfile: "ci",
      }),
    );

    const missing = referencesMatchingNothing(serialGroupReferences(config), workspace);

    expect(missing).toEqual(["tddy-daemon::session_room_livekit_acceptance"]);
  });

  test("an_unqualified_binary_matches_when_any_package_has_it", () => {
    const config = ciSerialisation(
      aConfigPutting("binary(session_room_livekit_acceptance)", { inGroup: "livekit", onProfile: "ci" }),
    );

    const missing = referencesMatchingNothing(serialGroupReferences(config), workspace);

    expect(missing).toEqual([]);
  });

  test("a_package_named_wholesale_must_be_a_workspace_member", () => {
    const config = ciSerialisation(aConfigPutting("package(tddy-carved-away)", { inGroup: "livekit", onProfile: "ci" }));

    const missing = referencesMatchingNothing(serialGroupReferences(config), workspace);

    expect(missing).toEqual(["package(tddy-carved-away)"]);
  });
});

// ---------------------------------------------------------------------------------------------
// Discovering test binaries
// ---------------------------------------------------------------------------------------------

describe("test binary discovery", () => {
  test("a_tests_dir_main_rs_target_is_named_after_its_dir", () => {
    const workspace = aWorkspace().withPackage("tddy-livekit", { "tests/room_roster/main.rs": "fn main() {}\n" });

    const binaries = testBinaries(workspace.root);

    expect(binaryNames(binaries)).toEqual(["tddy-livekit::room_roster"]);
  });

  test("a_manifest_with_explicit_test_targets_is_rejected", () => {
    const workspace = aWorkspace().withPackage(
      "tddy-livekit",
      { "tests/room_roster.rs": "" },
      '\n[[test]]\nname = "room_roster"\npath = "tests/room_roster.rs"\n',
    );

    expect(() => testBinaries(workspace.root)).toThrow(/declares \[\[test\]\] targets/);
  });

  test("a_manifest_that_sets_autotests_is_rejected", () => {
    const workspace = aWorkspace().withPackage("tddy-livekit", { "tests/room_roster.rs": "" }, "autotests = false\n");

    expect(() => testBinaries(workspace.root)).toThrow(/sets autotests/);
  });

  test("a_package_dir_that_is_not_a_workspace_member_is_not_discovered", () => {
    const workspace = aWorkspace()
      .withPackage("tddy-livekit", { "tests/room_roster.rs": "" })
      .withNonMember("tddy-scratch", { "tests/scratch.rs": "" });

    const binaries = testBinaries(workspace.root);

    expect(binaryNames(binaries)).toEqual(["tddy-livekit::room_roster"]);
  });

  test("a_module_in_a_mod_rs_file_counts_as_part_of_the_binary_declaring_it", () => {
    const workspace = aWorkspace().withPackage("tddy-livekit", {
      "tests/room_roster.rs": "mod common;\n",
      "tests/common/mod.rs": STARTS_THE_TESTKIT,
    });

    const starting = binariesStartingTheTestkit(workspace.root);

    expect(binaryNames(starting)).toContain("tddy-livekit::room_roster");
  });

  test("a_module_of_a_module_resolves_under_its_parents_dir", () => {
    const workspace = aWorkspace().withPackage("tddy-livekit", {
      "tests/room_roster/main.rs": "mod common;\n",
      "tests/room_roster/common.rs": "pub mod server;\n",
      "tests/room_roster/common/server.rs": STARTS_THE_TESTKIT,
    });

    const starting = binariesStartingTheTestkit(workspace.root);

    expect(binaryNames(starting)).toEqual(["tddy-livekit::room_roster"]);
  });

  test("a_mod_declaration_inside_a_string_literal_is_not_a_module", () => {
    const workspace = aWorkspace().withPackage("tddy-livekit", {
      "tests/room_roster.rs": 'const FIXTURE: &str = "\\\n    pub mod fixture_only;\n";\n',
    });

    const binaries = testBinaries(workspace.root);

    expect(binaries[0].sources).toHaveLength(1);
  });

  test("a_declared_module_without_a_file_is_rejected", () => {
    const workspace = aWorkspace().withPackage("tddy-livekit", { "tests/room_roster.rs": "mod missing;\n" });

    expect(() => testBinaries(workspace.root)).toThrow(/`mod missing;` .* found neither of missing\.rs and missing\/mod\.rs/);
  });
});
