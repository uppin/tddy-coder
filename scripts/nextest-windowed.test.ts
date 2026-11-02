import { describe, expect, test } from "bun:test";
import {
  type WindowPackage,
  balancedBatches,
  parseShard,
  shardPackages,
  testBinaryPaths,
  windowBatches,
} from "./nextest-windowed";

function aPackage(name: string, binaries: number): WindowPackage {
  return { name, binaries };
}

describe("the window batches", () => {
  test("packages_fill_a_batch_up_to_the_binary_cap_in_workspace_order", () => {
    // Given
    const packages = [aPackage("a", 10), aPackage("b", 20), aPackage("c", 15), aPackage("d", 5)];

    // When
    const batches = windowBatches(packages, 30);

    // Then
    expect(batches).toEqual([["a", "b"], ["c", "d"]]);
  });

  test("a_package_over_the_cap_is_a_batch_of_its_own_rather_than_split_or_skipped", () => {
    // Given
    const packages = [aPackage("small", 5), aPackage("huge", 99), aPackage("tail", 5)];

    // When
    const batches = windowBatches(packages, 30);

    // Then
    expect(batches).toEqual([["small"], ["huge"], ["tail"]]);
  });

  test("every_package_lands_in_exactly_one_batch", () => {
    // Given
    const packages = Array.from({ length: 40 }, (_, i) => aPackage(`p${i}`, (i * 7) % 23 + 1));

    // When
    const batches = windowBatches(packages, 50);

    // Then
    expect(batches.flat()).toEqual(packages.map((p) => p.name));
  });

  test("a_package_with_no_test_binaries_is_left_out", () => {
    // Given
    const packages = [aPackage("a", 3), aPackage("no-tests", 0), aPackage("b", 3)];

    // When
    const batches = windowBatches(packages, 10);

    // Then
    expect(batches).toEqual([["a", "b"]]);
  });

  test("a_cap_below_one_is_refused", () => {
    expect(() => windowBatches([aPackage("a", 1)], 0)).toThrow(/cap/);
  });
});

describe("a fixed number of batches", () => {
  test("splits_the_packages_into_that_many_contiguous_batches_of_about_equal_binaries", () => {
    // Given
    const packages = [aPackage("a", 10), aPackage("b", 10), aPackage("c", 10), aPackage("d", 10)];

    // When
    const batches = balancedBatches(packages, 2);

    // Then
    expect(batches).toEqual([["a", "b"], ["c", "d"]]);
  });

  test("never_makes_more_batches_than_asked_and_keeps_every_package_once", () => {
    // Given
    const packages = Array.from({ length: 109 }, (_, i) => aPackage(`p${i}`, (i * 13) % 31 + 1));

    // When
    const batches = balancedBatches(packages, 2);

    // Then
    expect(batches).toHaveLength(2);
    expect(batches.flat()).toEqual(packages.map((p) => p.name));
  });

  test("balances_by_binaries_not_by_package_count", () => {
    // Given one heavy package up front
    const packages = [aPackage("heavy", 90), aPackage("x", 5), aPackage("y", 5), aPackage("z", 100)];

    // When
    const batches = balancedBatches(packages, 2);

    // Then the first batch closes once it holds half the binaries
    expect(batches).toEqual([["heavy", "x", "y"], ["z"]]);
  });

  test("fewer_packages_than_batches_gives_one_batch_per_package", () => {
    expect(balancedBatches([aPackage("a", 3), aPackage("b", 4)], 5)).toEqual([["a"], ["b"]]);
  });

  test("a_batch_count_below_one_is_refused", () => {
    expect(() => balancedBatches([aPackage("a", 1)], 0)).toThrow(/batch count/);
  });
});

describe("the test binaries a batch built", () => {
  // The shape `cargo nextest list --list-type binaries-only --message-format json` prints, with the
  // dev shell's banner line ahead of it on stdout.
  const LISTING = `tddy-coder dev shell: rustc, cargo, rustfmt, clippy, rust-analyzer, bun, node, rg
{"rust-build-meta":{"target-directory":"/w/target","non-test-binaries":{"tddy-tools":[{"name":"tddy-tools","path":"debug/tddy-tools"}]}},
 "rust-binaries":{
  "tddy-e2e":{"binary-id":"tddy-e2e","kind":"lib","binary-path":"/w/target/debug/deps/tddy_e2e-41873552bc17631f"},
  "tddy-e2e::web_dev_script":{"binary-id":"tddy-e2e::web_dev_script","kind":"test","binary-path":"/w/target/debug/deps/web_dev_script-0a1b2c3d4e5f6a7b"}
 }}`;

  test("are_the_rust_binaries_nextest_lists_and_never_its_non_test_binaries", () => {
    // When
    const paths = testBinaryPaths(LISTING);

    // Then
    expect(paths).toEqual([
      "/w/target/debug/deps/tddy_e2e-41873552bc17631f",
      "/w/target/debug/deps/web_dev_script-0a1b2c3d4e5f6a7b",
    ]);
  });

  test("a_listing_without_json_is_an_error_not_an_empty_list", () => {
    expect(() => testBinaryPaths("tddy-coder dev shell: …\nerror: could not compile")).toThrow(/listing/);
  });
});

describe("a shard of the workspace", () => {
  const workspace = Array.from({ length: 109 }, (_, i) => aPackage(`p${i}`, ((i * 13) % 31) + 1));
  const names = (pkgs: WindowPackage[]) => pkgs.map((p) => p.name);

  test("the_shards_together_hold_every_package_with_tests_and_never_overlap", () => {
    // When
    const shards = [1, 2, 3].map((i) => shardPackages(workspace, { index: i, total: 3 }));

    // Then
    const all = shards.flatMap(names);
    expect(all.slice().sort()).toEqual(names(workspace).sort());
    expect(new Set(all).size).toBe(all.length);
  });

  test("a_package_with_no_test_binaries_is_in_no_shard", () => {
    // Given
    const packages = [aPackage("a", 3), aPackage("none", 0), aPackage("b", 3)];

    // When
    const all = [1, 2].flatMap((i) => names(shardPackages(packages, { index: i, total: 2 })));

    // Then
    expect(all).toEqual(["a", "b"]);
  });

  test("the_shards_are_balanced_by_test_binaries_to_within_the_largest_package", () => {
    // When
    const totals = [1, 2, 3, 4].map((i) =>
      shardPackages(workspace, { index: i, total: 4 }).reduce((sum, p) => sum + p.binaries, 0),
    );

    // Then
    const largest = Math.max(...workspace.map((p) => p.binaries));
    expect(Math.max(...totals) - Math.min(...totals)).toBeLessThanOrEqual(largest);
  });

  test("the_split_is_deterministic_and_keeps_workspace_order", () => {
    // When
    const first = shardPackages(workspace, { index: 2, total: 3 });
    const again = shardPackages(workspace.slice(), { index: 2, total: 3 });

    // Then
    expect(again).toEqual(first);
    const positions = names(first).map((n) => names(workspace).indexOf(n));
    expect(positions).toEqual(positions.slice().sort((a, b) => a - b));
  });

  test("a_shard_left_empty_is_an_error_not_a_silent_pass", () => {
    // Given two packages cannot fill three shards
    const packages = [aPackage("a", 3), aPackage("b", 4)];

    // Then
    expect(() => shardPackages(packages, { index: 3, total: 3 })).toThrow(/empty|fewer/);
  });

  test("a_shard_out_of_range_is_refused", () => {
    expect(() => shardPackages(workspace, { index: 0, total: 3 })).toThrow(/shard/);
    expect(() => shardPackages(workspace, { index: 4, total: 3 })).toThrow(/shard/);
  });

  test("parses_i_over_n", () => {
    expect(parseShard("2/3")).toEqual({ index: 2, total: 3 });
  });

  test("a_malformed_shard_is_refused", () => {
    for (const bad of ["", "3", "1/", "/3", "a/b", "1/2/3", "1.5/3", "-1/3", "0/3", "4/3", "1/0"]) {
      expect(() => parseShard(bad)).toThrow(/shard/);
    }
  });
});
