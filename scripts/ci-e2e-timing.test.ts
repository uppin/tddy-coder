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

/** The shape cargo writes into `cargo-timing.html`: one object per compiled unit. */
function aTimingsReport(units: Array<{ name: string; target: string; seconds: number }>): string {
  const data = units.map((unit, i) => ({
    i,
    name: unit.name,
    version: "0.1.0",
    mode: unit.target.includes("(test") ? "test" : "build",
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
    { name: "tddy-daemon", target: ' (test "multi_host_acceptance")', seconds: 20 },
    { name: "tddy-livekit", target: ' (test "rpc_scenarios")', seconds: 10 },
    { name: "tddy-core", target: ' (test "plain_unit_binary")', seconds: 15 },
    { name: "tddy-core", target: ' (test "another_unit_binary")', seconds: 5 },
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
});

describe("verdict", () => {
  test("the_verdict_says_proceed_at_or_above_twenty_five_percent_and_stop_below", () => {
    expect(verdict(25)).toBe("proceed");
    expect(verdict(24.9)).toBe("stop");
  });
});
