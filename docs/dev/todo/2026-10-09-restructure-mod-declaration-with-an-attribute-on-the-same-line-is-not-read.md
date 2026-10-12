# 2026-10-09: `#[cfg(…)] mod x;` written on one line is not recognised as a declaration

**Category:** Restructure engine defect
**Source:** #reshape 5/19 (`feature/reshape/move-children`), found while reading `crate_move/manifest_edits.rs`

## What happens

`manifest_edits::declared_module` (#reshape 5) and `module_declaration` read a line that starts with an optional visibility and then `mod <name>;`. A declaration written with its attribute on the same line, such as `#[cfg(unix)] mod x;` or `#[path = "p.rs"] mod x;`, is not matched. A move of `x` is then refused as "declares no `mod x`", which is a misleading message. #reshape 3 (`tidy-facades`) refuses a declaration carrying an attribute on the line *above* it, naming the attribute. The same-line form never reaches that refusal.

## What the engine should do

Recognise the same-line form and hand it to #reshape 3's attribute refusal, so the message names the attribute.

## Why deferred

It is not seen in any run. Fixing it overlaps #reshape 3's span work and is cheapest after both nodes have landed.
