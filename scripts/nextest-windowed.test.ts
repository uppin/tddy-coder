import { describe, expect, test } from "bun:test";
import { type WindowPackage, testBinaryPaths, windowBatches } from "./nextest-windowed";

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
