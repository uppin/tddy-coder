# oversized-file: pr_api.rs

**Location:** `packages/tddy-github/src/pr_api.rs`
**Category:** oversized-file
**Detected:** 2026-09-22 by `/pr-wrap` step 3.5 on #492, measured by hand
**Metrics:** **≈ 915 production lines** of 1,292 total · the budget gate reports **315** · 2 test modules, 379 lines
**Restructure:** required — split by trait, `/code-restructuring` territory
**Status:** Open
**Moved:** was `packages/tddy-workflow-recipes/src/orchestrate_pr_stack/github.rs` — relocated byte-identical by #492 (`#carve` 6/11)

## Measurement history

| Run | Production lines | Gate count | Note |
|---|---|---|---|
| 2026-09-22 | ≈ 915 | 315 | first detection, at the file's new home; unchanged by the move |

## What the tool found

The file-length gate counts production lines **to the first `#[cfg(test)]`**, and here that line is
316. But production code resumes after it:

| Lines | What |
|---|---|
| 1–315 | DTOs (`PrRef`, `PrState`, `PrView`, `PrDetail`, `PrFile`, `CheckRun`, `PrReview`, …), the `GithubPrApi` and `GithubPrInsightApi` traits, `RealGithubPrApi` and its constructor |
| 316–630 | `#[cfg(test)] mod tests` |
| 631–654 | `owner_repo_from_remote_url` — production |
| 655–718 | `#[cfg(test)] mod real_impl_tests` |
| 719–910 | `impl GithubPrApi for RealGithubPrApi` — production |
| 911–948 | JSON helpers — production |
| 949–1292 | `impl GithubPrInsightApi for RealGithubPrApi`, `scoped_value`, `search_qualifiers` — production |

So the gate reads 315 and the file really carries about 915. **This is not a defect in the gate** —
it measures the repo's convention — but it means a file laid out like this one is invisible to it,
and "under budget" here would be false.

## Why it matters here

`pr_api` is the PR-stack's whole view of GitHub: every repoint, merge, adoption and insight call goes
through these two traits. It is the file `#carve` 10/11 (`pr-stack-crate`) consumes, and it grows
every time the PR-stack surface asks GitHub something new.

## What would close it

Two cuts, both along seams the file already has:

1. **Layout first, which is free:** move `owner_repo_from_remote_url` and both `impl` blocks above
   the test modules, so the gate measures what is really there. Pure reorder.
2. **Then split by trait:** DTOs + traits (`pr_api/types.rs` or the root), `impl GithubPrApi`
   (`pr_api/lifecycle.rs` — open, merge, repoint, create, close), `impl GithubPrInsightApi` and its
   JSON helpers (`pr_api/insight.rs` — files, checks, reviews, comments, search). Each lands well
   under 500. `pub use` at `pr_api`'s root keeps every path.

## Verified by hand

2026-09-22: outlined top-level items and `#[cfg(test)]` gates with `awk`; confirmed both
`impl … for RealGithubPrApi` blocks are outside any test module; subtracted the two test modules'
line spans from the total. The byte-identity of the move was checked with
`git show HEAD:<old path> | diff - <new path>` (empty), so the size is inherited, not grown.
