# 2026-09-16 — Complexity cached by content, and an error class for a path that is not a crate

**Type:** Enhancement

`file_complexity` is pure over the source text, so a long-lived host can keep each score against a
hash of the text it scored. `ComplexityCache` is the seam: `InMemoryComplexityCache` for a process
that serves many requests, `PassThroughComplexityCache` for the command line, which keeps nothing and
therefore behaves exactly as it always has. Keyed by content rather than by path, so two files with
identical contents are scored once and a file whose contents changed is always rescored — a path says
nothing about whether the answer is still true. The digest is md5, already used in this crate for
content addressing; no dependency was added.

`generate_report` takes the cache, and its progress line moved to the command line so the library
prints nothing.

`AnalysisError::NotACrate { path }` replaces the one `Message("no Cargo.toml at …")`, so a path that
is not a crate refuses as a caller error rather than landing in the unclassified class. `Message`
remains mapped to an internal class deliberately: an unclassified refusal is exactly one a caller
cannot act on, and the honest fix for a mis-classified one is a variant rather than a cleverer
default.

The in-memory cache is unbounded — see the backlog entry of that name.
