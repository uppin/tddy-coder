# 2026-09-09 — `analyze duplicate-tests` is quadratic over per-test signatures

**Category:** Future enhancement
**Source:** `analyze-coverage-export-and-harness-selection` (#466), first full `tddy-daemon` run

Detecting subset and identical signatures over **2,159** captured tests took **~22 minutes** — longer
per test than the capture that produced them, which now runs the whole daemon suite in 55.8 min. The
comparison is pairwise (~2.3M pairs), and each pair compares region sets, so the cost grows with the
square of the suite while the capture itself is linear.

It is the same class of problem [targeted instrumentation](../changesets/2026-09-09-analyze-coverage-targeted-instrumentation.md)
just removed from `capture_coverage`, and it was left alone deliberately: #466 was already three
commits of capture work, and nothing about the duplicate-tests output was *wrong* — only slow. It
becomes the bottleneck the moment anyone runs the full pipeline routinely.

Worth trying before rewriting the algorithm: hash each signature and bucket by size first, so only
candidates within a plausible containment ratio are compared at all. Most of the 2.3M pairs are
between tests whose region counts differ by an order of magnitude and can never be subsets.
