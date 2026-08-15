#!/usr/bin/env bash
# Report GitHub Actions CI status for a pull request, in a form an agent can act on.
#
#   scripts/ci-status.sh                 # current branch's PR: one line per check, plus test counts
#   scripts/ci-status.sh 396             # a specific PR number
#   scripts/ci-status.sh --failures      # also print failing test names and the tail of failing job logs
#   scripts/ci-status.sh 396 --failures
#   scripts/ci-status.sh --watch         # block until the run finishes, then report
#
# The test counts and the failing test names come from the check runs published
# by mikepenz/action-junit-report (see .github/workflows/ci.yml), so a red check
# tells you *which tests* failed without downloading or parsing any artifact.
#
# Requires: gh, authenticated with at least the `repo` scope.
set -euo pipefail

PR=""
SHOW_FAILURES=0
WATCH=0

for arg in "$@"; do
  case "$arg" in
    --failures) SHOW_FAILURES=1 ;;
    --watch) WATCH=1 ;;
    -h | --help)
      awk 'NR>1 && /^#/ { sub(/^# ?/, ""); print; next } NR>1 { exit }' "$0"
      exit 0
      ;;
    [0-9]*) PR="$arg" ;;
    *)
      echo "unknown argument: $arg (try --help)" >&2
      exit 2
      ;;
  esac
done

command -v gh >/dev/null || {
  echo "gh is not installed" >&2
  exit 1
}

REPO="$(gh repo view --json nameWithOwner -q .nameWithOwner)"

if [[ -z "$PR" ]]; then
  PR="$(gh pr view --json number -q .number 2>/dev/null)" || {
    echo "No PR for the current branch. Pass a PR number explicitly." >&2
    exit 1
  }
fi

SHA="$(gh pr view "$PR" --json headRefOid -q .headRefOid)"
TITLE="$(gh pr view "$PR" --json title -q .title)"

echo "PR #$PR  $TITLE"
echo "repo: $REPO   head: ${SHA:0:12}"
echo

if [[ "$WATCH" == 1 ]]; then
  # --watch exits non-zero when a check fails; that is a report, not an error here.
  gh pr checks "$PR" --watch || true
  echo
fi

echo "── checks ─────────────────────────────────────────────"
gh pr checks "$PR" || true
echo

# The junit-report action puts the pass/fail tally in the check run's output
# title, e.g. "3217 tests run, 3214 passed, 3 failed, 0 skipped".
echo "── test totals ────────────────────────────────────────"
gh api "repos/$REPO/commits/$SHA/check-runs" --paginate \
  --jq '.check_runs[] | select(.output.title != null) | "\(.name): \(.output.title)"' \
  || echo "(no check runs with test output yet)"
echo

if [[ "$SHOW_FAILURES" != 1 ]]; then
  echo "Re-run with --failures for failing test names and log tails."
  exit 0
fi

echo "── failing tests ──────────────────────────────────────"
# Each failed test becomes an annotation carrying its name, file, line and the
# assertion message.
FAILED_CHECK_IDS="$(gh api "repos/$REPO/commits/$SHA/check-runs" --paginate \
  --jq '.check_runs[] | select(.conclusion == "failure") | .id')"

if [[ -z "$FAILED_CHECK_IDS" ]]; then
  echo "(no failed check runs)"
else
  for id in $FAILED_CHECK_IDS; do
    gh api "repos/$REPO/check-runs/$id/annotations" --paginate \
      --jq '.[] | "\(.path):\(.start_line)  \(.title // "")\n\(.message)\n"' || true
  done
fi
echo

echo "── failing job logs (tail) ────────────────────────────"
RUN_IDS="$(gh api "repos/$REPO/actions/runs?head_sha=$SHA&per_page=20" \
  --jq '.workflow_runs[] | select(.conclusion == "failure") | .id')"

if [[ -z "$RUN_IDS" ]]; then
  echo "(no failed workflow runs)"
else
  for run in $RUN_IDS; do
    echo "--- run $run"
    # --log-failed prints only the steps that failed.
    gh run view "$run" --log-failed 2>/dev/null | tail -n 120 || true
    echo
  done
fi
