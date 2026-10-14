# 2026-10-13 — facades keep a public path, crate moves remove a declaration whole, and the tidy gates what only tests read

Feature: [Rust code restructuring](../rust-code-restructuring.md#import-restoration).

Five defects each cost a hand edit after an otherwise clean `apply`.

- **A facade keeps the public path.** An `extract_module` reads the file's outline after the server has
  loaded, and a range that declares items while the survey found none is refused, naming the range,
  instead of a facade written from nothing. A `named` facade re-exports every moved `pub` item whether
  or not anything references it; a narrower item only when something outside the module reaches it.
- **Names only tests reach are gated.** Under a `#[cfg(test)]` line after the production lines, so the
  tidy has nothing to rewrite. A reference from another file counts as production.
- **A crate move removes a declaration whole.** The doc and plain comments directly above `mod <module>;`
  leave the origin and are written above the destination's `pub mod <module>;`. A declaration carrying an
  attribute (`#[cfg]`, `#[path]`, `#[allow]`) is refused by `check` and by `apply`, naming it, before
  anything is written.
- **A crate move keeps `use` runs apart.** Where the removed declaration was the only line between two
  runs, one blank line stays, so `rustfmt` does not re-sort unrelated lines.
- **The tidy gates a glob, or a trait import, that only tests read** instead of failing the run over a
  tree that compiles. A glob no build reads is removed as before.
- **A failed compile gate says the tidy did not run**, and how many `unused import` warnings the failing
  check reported in the written files.
- **Two plans can be refused that ran before:** an empty survey over declared items, and an attributed
  crate-moved declaration. Both say what to do.
- **Still by hand:** a failed gate neither removes the unused imports nor formats the files; a
  `reparent_module` or `move_item` removal between two `use` runs can still join them; a `move_item`
  facade path through `super::<crate>::` is still miswritten.
