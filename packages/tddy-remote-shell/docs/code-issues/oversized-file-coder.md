# oversized-file: coder.rs

**Location:** `packages/tddy-remote-shell/src/coder.rs`
**Category:** oversized-file
**Detected:** 2026-11-18 by structural audit (`/yolo-pr-stack` sweep-collect, `#coder-start`) — production lines counted before the first `#[cfg(test)]`: `awk '/^#\[cfg\(test\)\]/{exit} END{print NR-1}' packages/tddy-remote-shell/src/coder.rs`
**Metrics:** **507 production lines** (`#[cfg(test)]` at line 508; 619 lines in all) · budget 500 · 7 over
**Restructure:** `move_item` / `extract_module --to_file` of the one-shot stdin transport (`STDIN_LINE_WIDTH`, `STDIN_NOT_WHOLE_STATUS`, `stdin_lines`, `one_shot_command`, `holds_the_start_marker_line`, `read_noting_the_marker`, ~95 lines, all free functions with no `self`) into a `coder/one_shot.rs` sibling — `/code-restructuring` territory
**Status:** Open — added by the `#coder-start` stack (#939 → #941 → #942), **unclaimed**

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-18 | 340 → 507 | first detection. 340 on merge base `2ed59a3c7`, 507 on `6e88f90e6`: the Coder one-shot pty fix (stdin as counted base64 lines, `stty -opost`, marker-gated feed) added about 167 lines, and a previous trim kept the file at 507 rather than lower |

## What the tool found

A line count, not an analyzer score. `tddy-tools analyze` ran on the package (CRAP join 73.9%, 184 of 249 functions): no function in the file scores above 8 (`strip_start_marker`, complexity 7, CRAP 7.0; the new `read_noting_the_marker` 5; `exec` 5), so the problem is the file's size, not any function's.

## Why it matters here

The next change to the Coder shell lands in a file already over budget, with the stdin transport interleaved between the marker helpers and `CoderStdin`.

## What would close it

The six items above are pure functions and two constants with one caller (`CoderShell::exec`); moving them to a sibling module takes the file to about 410 production lines. Ordinary move, no signature change.

## Verified by hand

2026-11-18: opened the file; the 507 count is real (`#[cfg(test)]` at 508). The moved block has no `self` and no shared state beyond `START_MARKER`, which `holds_the_start_marker_line` and `strip_start_marker` both read, so that constant moves to the sibling with them or is imported.
