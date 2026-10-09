# 2026-10-09 — `move_test_binary_to_crate` reads no target-specific dependency table

**Category:** Restructure engine defect (deferred)
**Source:** #reshape 9/19 (`feature/reshape/new-crate`), decision F5

`#reshape` 9 taught the module and cluster crate moves to read `[target.'<cfg>'.dependencies]` and
`[target.'<cfg>'.dev-dependencies]` (`crate_move/manifest_edits.rs` `target_declarations`,
`declares_dependency_in_any_table`, `with_lines_under`), so a crate the origin declares only for a target (`libc` under
`cfg(unix)`) is surveyed and carried into the same table of the destination.

`move_test_binary_to_crate` has its own crate-head scan and dev-dependency pass (`crate_move/test_binary.rs`,
`crate_shaped_heads`, `destination_dev_dependencies`, `dev_dependency_line_for`), and it still reads only `[dependencies]`
and `[dev-dependencies]`. So a moved test that names a target-only crate either has that crate ignored, if the path is
treated as not crate-shaped, or is refused as "declared nowhere". A test using `libc::` on a crate that declares it under
`[target.'cfg(unix)'.dev-dependencies]` reproduces it.

**Why deferred.** `test_binary.rs` is the 967-line file `#reshape` 15 splits (`oversized-file-test-binary.md`). Editing
its scanner in a parallel wave-1 node would collide with that extraction, and no test-binary move in the backlog names a
target-only crate.

**What would close it.** Route `dev_dependency_line_for` through `manifest_edits::target_declarations`, falling back to
the target `dependencies` table as the module pass does, and write the line with `with_lines_under` under the same
header. Then add a test-binary twin of `#reshape` 9's `a_crate_only_test_code_names_lands_in_the_target_dev_dependencies_table`.
Best done after `#reshape` 15 has extracted the shared scanner.
