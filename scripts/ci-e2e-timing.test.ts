import { describe, expect, test } from "bun:test";
import { compileShare, perBinaryTimings, verdict } from "./ci-e2e-timing";

const JUNIT = `<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nextest-run" tests="4" time="12.5">
  <testsuite name="tddy-daemon::multi_host_acceptance" tests="2" time="9.0">
    <testcase name="a_one" classname="tddy-daemon::multi_host_acceptance" time="4.0"/>
    <testcase name="a_two" classname="tddy-daemon::multi_host_acceptance" time="5.0"/>
  </testsuite>
  <testsuite name="tddy-livekit::rpc_scenarios" tests="1" time="2.5">
    <testcase name="rpc_scenarios" classname="tddy-livekit::rpc_scenarios" time="2.5"/>
  </testsuite>
  <testsuite name="tddy-livekit::rpc_client_factory" tests="1" time="1.0">
    <testcase name="basic" classname="tddy-livekit::rpc_client_factory" time="1.0"/>
  </testsuite>
</testsuites>`;

/**
 * The shape cargo writes into `cargo-timing.html`, taken from a real `--timings` report: one object
 * per compiled unit, `mode` is `todo` for a compile, and a test unit is marked by its target string.
 */
function aTimingsReport(units: Array<{ name: string; target: string; seconds: number }>): string {
  const data = units.map((unit, i) => ({
    i,
    name: unit.name,
    version: "0.1.0",
    mode: "todo",
    target: unit.target,
    features: [],
    duration: unit.seconds,
  }));
  return `<html><script>const UNIT_DATA = ${JSON.stringify(data)};</script></html>`;
}

const FILTERSET = `binary(multi_host_acceptance)
 or binary(rpc_scenarios)
 or package(tddy-supervisor)`;

describe("per-binary timings", () => {
  test("the_per_binary_table_is_sorted_by_time_and_totals_the_binaries", () => {
    const table = perBinaryTimings(JUNIT);

    expect(table.map((row) => row.binary)).toEqual([
      "multi_host_acceptance",
      "rpc_scenarios",
      "rpc_client_factory",
    ]);
    expect(table[0]).toEqual({ binary: "multi_host_acceptance", seconds: 9, tests: 2 });
    expect(table.reduce((sum, row) => sum + row.seconds, 0)).toBe(12.5);
  });

  test("a_junit_file_without_test_cases_reports_an_empty_table_not_a_crash", () => {
    expect(perBinaryTimings(`<testsuites name="nextest-run" tests="0" time="0"/>`)).toEqual([]);
  });
});

describe("compile share", () => {
  const report = aTimingsReport([
    { name: "tokio", target: "", seconds: 50 },
    { name: "tddy-daemon", target: ' test "multi_host_acceptance" (test)', seconds: 20 },
    { name: "tddy-livekit", target: ' test "rpc_scenarios" (test)', seconds: 10 },
    { name: "tddy-core", target: ' test "plain_unit_binary" (test)', seconds: 15 },
    { name: "tddy-core", target: ' test "another_unit_binary" (test)', seconds: 5 },
  ]);

  test("the_compile_share_counts_only_test_targets_named_by_the_filterset", () => {
    const share = compileShare(report, FILTERSET);

    expect(share.e2eTestSeconds).toBe(30);
    expect(share.otherTestSeconds).toBe(20);
    expect(share.otherSeconds).toBe(50);
    expect(share.totalSeconds).toBe(100);
    expect(share.e2eLegSkippablePercent).toBe(20);
    expect(share.unitLegSkippablePercent).toBe(30);
  });

  test("a_filterset_binary_missing_from_the_report_is_named_in_the_output", () => {
    const share = compileShare(report, `${FILTERSET}\n or binary(renamed_away_acceptance)`);

    expect(share.missingFromReport).toContain("renamed_away_acceptance");
  });

  test("a_package_and_binary_filterset_term_counts_only_that_binary_of_that_package", () => {
    const twoPackagesOneName = aTimingsReport([
      { name: "tddy-daemon", target: ' test "first_login_enrolment_acceptance" (test)', seconds: 10 },
      { name: "tddy-daemon", target: ' test "unrelated_daemon_binary" (test)', seconds: 30 },
      { name: "tddy-core", target: ' test "first_login_enrolment_acceptance" (test)', seconds: 5 },
    ]);

    const share = compileShare(
      twoPackagesOneName,
      "package(tddy-daemon) and binary(first_login_enrolment_acceptance)",
    );

    expect(share.e2eTestSeconds).toBe(10);
    expect(share.otherTestSeconds).toBe(35);
  });

  test("a_librarys_unit_tests_and_a_build_script_run_are_not_integration_targets", () => {
    const share = compileShare(
      aTimingsReport([
        { name: "tddy-supervisor", target: " lib (test)", seconds: 8 },
        { name: "tddy-core", target: " build script (run)", seconds: 4 },
      ]),
      "package(tddy-supervisor) and kind(test)",
    );

    expect(share.e2eTestSeconds).toBe(8);
    expect(share.otherSeconds).toBe(4);
  });

  test("a_report_without_units_has_no_time_to_share_and_names_every_filterset_binary", () => {
    const share = compileShare(aTimingsReport([]), "binary(rpc_scenarios)");

    expect(share.totalSeconds).toBe(0);
    expect(share.e2eLegSkippablePercent).toBe(0);
    expect(share.missingFromReport).toEqual(["rpc_scenarios"]);
  });

  test("a_filterset_predicate_it_cannot_evaluate_is_an_error_not_a_guess", () => {
    expect(() => compileShare(report, "test(some_name)")).toThrow(/unsupported filterset predicate/);
  });
});

describe("verdict", () => {
  test("the_verdict_says_proceed_at_or_above_twenty_five_percent_and_stop_below", () => {
    expect(verdict(25)).toBe("proceed");
    expect(verdict(24.9)).toBe("stop");
  });
});
