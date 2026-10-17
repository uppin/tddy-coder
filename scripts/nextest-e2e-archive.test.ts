import { describe, expect, test } from "bun:test";
import { e2eArchivePlan, unitTestBinaries } from "./nextest-e2e-archive";
import type { FilterSubject } from "./nextest-filterset";
import type { TestTarget } from "./nextest-serial-groups";

function aTarget(pkg: string, binary: string): TestTarget {
  return { package: pkg, binary, sources: [] };
}

function aUnitBinary(pkg: string, binary = pkg): FilterSubject {
  return { package: pkg, binary, unit: true };
}

const WORKSPACE: TestTarget[] = [
  aTarget("tddy-daemon", "multi_host_acceptance"),
  aTarget("tddy-daemon", "plain_daemon_test"),
  aTarget("tddy-livekit", "rpc_scenarios"),
  aTarget("tddy-supervisor", "supervisor_lifecycle"),
  aTarget("tddy-supervisor", "supervisor_cgroups"),
  aTarget("tddy-core", "plain_unit_binary"),
];
const UNIT_BINARIES = [aUnitBinary("tddy-daemon"), aUnitBinary("tddy-supervisor"), aUnitBinary("tddy-core")];

describe("the e2e archive arguments", () => {
  test("a_binary_term_archives_that_test_target_in_its_package", () => {
    const plan = e2eArchivePlan("binary(multi_host_acceptance)", WORKSPACE, UNIT_BINARIES);

    expect(plan.args).toEqual(["-p", "tddy-daemon", "--test", "multi_host_acceptance"]);
  });

  test("a_package_and_kind_test_term_archives_every_integration_test_of_the_package", () => {
    const plan = e2eArchivePlan("package(tddy-supervisor) and kind(test)", WORKSPACE, UNIT_BINARIES);

    expect(plan.tests).toEqual(["supervisor_cgroups", "supervisor_lifecycle"]);
    expect(plan.packages).toEqual(["tddy-supervisor"]);
  });

  test("a_package_qualified_binary_is_built_only_in_that_package", () => {
    const twoPackagesOneName = [aTarget("tddy-daemon", "shared_name"), aTarget("tddy-core", "shared_name")];

    const plan = e2eArchivePlan("package(tddy-daemon) and binary(shared_name)", twoPackagesOneName, []);

    expect(plan.packages).toEqual(["tddy-daemon"]);
  });

  test("the_arguments_are_sorted_and_unique_however_the_filterset_orders_its_terms", () => {
    const plan = e2eArchivePlan(
      "binary(rpc_scenarios) or binary(multi_host_acceptance) or binary(rpc_scenarios)",
      WORKSPACE,
      UNIT_BINARIES,
    );

    expect(plan.args).toEqual([
      "-p",
      "tddy-daemon",
      "-p",
      "tddy-livekit",
      "--test",
      "multi_host_acceptance",
      "--test",
      "rpc_scenarios",
    ]);
  });
});

describe("a filterset the archive cannot honour", () => {
  test("a_binary_that_matches_no_test_target_is_an_error_naming_it", () => {
    expect(() => e2eArchivePlan("binary(multi_host_acceptance) or binary(renamed_away)", WORKSPACE, UNIT_BINARIES)).toThrow(
      /binary\(renamed_away\), which matches no integration-test target/,
    );
  });

  test("a_binary_qualified_by_the_wrong_package_is_an_error", () => {
    expect(() => e2eArchivePlan("package(tddy-core) and binary(multi_host_acceptance)", WORKSPACE, [])).toThrow(
      /package\(tddy-core\) and binary\(multi_host_acceptance\)/,
    );
  });

  test("a_package_without_integration_tests_is_an_error", () => {
    expect(() => e2eArchivePlan("package(tddy-empty) and kind(test)", WORKSPACE, [])).toThrow(/package\(tddy-empty\)/);
  });

  test("a_package_term_without_kind_test_is_refused_because_it_would_select_unit_tests", () => {
    expect(() => e2eArchivePlan("package(tddy-supervisor)", WORKSPACE, UNIT_BINARIES)).toThrow(
      /unit-test binaries \(tddy-supervisor::tddy-supervisor\)/,
    );
  });

  test("a_binary_term_naming_a_bin_target_is_refused_because_it_selects_a_unit_binary", () => {
    const withBin = [...UNIT_BINARIES, aUnitBinary("tddy-daemon", "multi_host_acceptance")];

    expect(() => e2eArchivePlan("binary(multi_host_acceptance)", WORKSPACE, withBin)).toThrow(/unit-test binaries/);
  });

  test("a_filterset_that_selects_nothing_is_an_error_not_an_archive_of_everything", () => {
    expect(() => e2eArchivePlan("not binary(multi_host_acceptance) and package(tddy-nothing)", WORKSPACE, [])).toThrow(
      /matches no integration-test target|selects no integration-test target/,
    );
  });
});

describe("the repository's own e2e filterset", () => {
  test("every_term_of_rust_e2e_filterset_matches_a_test_target_and_no_unit_binary", async () => {
    const filterset = await Bun.file(".config/rust-e2e.filterset").text();
    const { testBinaries } = await import("./nextest-serial-groups");

    const plan = e2eArchivePlan(filterset, testBinaries(process.cwd()), unitTestBinaries(process.cwd()));

    expect(plan.tests.length).toBeGreaterThan(50);
  });
});
