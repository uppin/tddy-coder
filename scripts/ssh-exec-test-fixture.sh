#!/usr/bin/env bash
# Prepare the buildbox SSH exec fixture for acceptance tests (engine, core, daemon).
set -euo pipefail

_script_dir="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
_tddy_root="$(cd "$_script_dir/.." && pwd)"
export TDDY_SSH_EXEC_FIXTURE_ROOT="${TDDY_SSH_EXEC_FIXTURE_ROOT:-$_tddy_root/packages/tddy-tool-engine/tests/fixtures/buildbox-root}"
export TDDY_REAL_SSH="${TDDY_REAL_SSH:-$(command -v ssh)}"

_sess_dir="$TDDY_SSH_EXEC_FIXTURE_ROOT/home/dev/repo/.worktrees/sess"
mkdir -p "$_sess_dir"
printf '%s' 'from-the-target' >"$_sess_dir/hello.txt"
if [[ ! -d "$_sess_dir/.git" ]]; then
  git -C "$_sess_dir" init -q
  git -C "$_sess_dir" add hello.txt
  git -C "$_sess_dir" -c user.email=tddy-fixture@test -c user.name=tddy-fixture commit -qm init
fi

_fixture_bin="$_tddy_root/packages/tddy-tool-engine/tests/fixtures/bin"
chmod +x "$_fixture_bin/ssh" 2>/dev/null || true
export PATH="$_fixture_bin:$PATH"
