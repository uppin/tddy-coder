#!/usr/bin/env bash
# Regenerate the committed generated code, or check that it matches its protos.
#
#   scripts/generated-code.sh check [package...]   regenerate into a temp tree and diff (default)
#   scripts/generated-code.sh write [package...]   regenerate in place
#
# `check` is the CI gate: it exits non-zero when a committed *_pb.ts differs from
# what `buf generate` produces today, when a generated file was never committed,
# or when a committed file no longer corresponds to any proto.
#
# The directories under the gate and the invocations that produce them are listed
# in scripts/generated-code.manifest.
#
# Options:
#   --manifest <path>  read the directory list from somewhere else (default: the
#                      manifest beside this script)
#   --root <path>      resolve the manifest's package paths against this directory
#                      (default: the repo root). The toolchain always comes from
#                      the repo's own node_modules, whatever this is set to.
#   --show-diff        print the full unified diff of every differing file rather
#                      than a line count
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

MODE=check
MANIFEST="$SCRIPT_DIR/generated-code.manifest"
ROOT="$REPO_ROOT"
SHOW_DIFF=0
PACKAGE_FILTER=()

usage() {
  # The header comment above, minus the shebang and the leading '# '.
  awk 'NR > 1 { if ($0 !~ /^#/) exit; sub(/^# ?/, ""); print }' "${BASH_SOURCE[0]}"
}

while [ $# -gt 0 ]; do
  case "$1" in
    check | write)
      MODE="$1"
      shift
      ;;
    --manifest)
      MANIFEST="$2"
      shift 2
      ;;
    --root)
      ROOT="$2"
      shift 2
      ;;
    --show-diff)
      SHOW_DIFF=1
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    -*)
      echo "generated-code.sh: unknown option $1" >&2
      usage >&2
      exit 2
      ;;
    *)
      PACKAGE_FILTER+=("$1")
      shift
      ;;
  esac
done

if [ ! -f "$MANIFEST" ]; then
  echo "generated-code.sh: no manifest at $MANIFEST" >&2
  exit 2
fi

# Both tools are pinned by bun.lock, so everyone generates with the same versions.
# The nix shell also carries a `buf`, deliberately not used: a second version would
# be a second answer to "what should be committed".
#
# Bun installs a workspace package's own devDependencies under that package, so the
# binaries land in `packages/<name>/node_modules/.bin` and NOT at the root. Every
# `.bin` in the workspace goes on PATH, root included, rather than assuming a hoist
# that this workspace does not perform.
for bin_dir in "$REPO_ROOT"/node_modules/.bin "$REPO_ROOT"/packages/*/node_modules/.bin; do
  [ -d "$bin_dir" ] && PATH="$bin_dir:$PATH"
done
export PATH
for tool in buf protoc-gen-es; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "generated-code.sh: $tool not found in any node_modules/.bin under $REPO_ROOT, or on PATH." >&2
    echo "  Install the workspace dependencies first (see CLAUDE.md § Bun Workspace)." >&2
    exit 2
  fi
done

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

FILTER_COUNT=${#PACKAGE_FILTER[@]}

wanted() {
  local package="$1"
  if [ "$FILTER_COUNT" -eq 0 ]; then
    return 0
  fi
  local name candidate
  name="$(basename "$package")"
  for candidate in "${PACKAGE_FILTER[@]}"; do
    if [ "$candidate" = "$package" ] || [ "$candidate" = "$name" ]; then
      return 0
    fi
  done
  return 1
}

# The manifest lists one invocation per line and a package may own several, so
# collect them per package before generating: `write` clears the gen directory
# once, and `check` builds one temp tree the whole package's output lands in.
# Three parallel arrays rather than one associative array, so this still runs
# under the bash 3.2 that macOS ships.
PACKAGES=()
GEN_DIRS=()
PACKAGE_ARGS=()

index_of() {
  local package="$1" i=0
  while [ "$i" -lt "${#PACKAGES[@]}" ]; do
    if [ "${PACKAGES[$i]}" = "$package" ]; then
      echo "$i"
      return 0
    fi
    i=$((i + 1))
  done
  return 1
}

while IFS= read -r line; do
  line="${line%%#*}"
  package="$(echo "${line%%|*}" | xargs)"
  [ -n "$package" ] || continue
  rest="${line#*|}"
  gen_dir="$(echo "${rest%%|*}" | xargs)"
  gen_args="$(echo "${rest#*|}" | xargs)"
  wanted "$package" || continue
  if existing="$(index_of "$package")"; then
    PACKAGE_ARGS[$existing]="${PACKAGE_ARGS[$existing]}"$'\n'"$gen_args"
  else
    PACKAGES[${#PACKAGES[@]}]="$package"
    GEN_DIRS[${#GEN_DIRS[@]}]="$gen_dir"
    PACKAGE_ARGS[${#PACKAGE_ARGS[@]}]="$gen_args"
  fi
done <"$MANIFEST"

if [ "${#PACKAGES[@]}" -eq 0 ]; then
  echo "generated-code.sh: no packages selected" >&2
  exit 2
fi

generate_into() {
  local package="$1" output="$2" all_args="$3"
  local args
  while IFS= read -r args; do
    [ -n "$args" ] || continue
    # Word splitting is intended: the manifest holds buf's own arguments.
    # shellcheck disable=SC2086
    (cd "$ROOT/$package" && buf generate $args --output "$output")
  done <<<"$all_args"
}

if [ "$MODE" = write ]; then
  i=0
  while [ "$i" -lt "${#PACKAGES[@]}" ]; do
    package="${PACKAGES[$i]}"
    gen_dir="${GEN_DIRS[$i]}"
    rm -rf "${ROOT:?}/$package/$gen_dir"
    generate_into "$package" "$ROOT/$package" "${PACKAGE_ARGS[$i]}"
    echo "regenerated $package/$gen_dir"
    i=$((i + 1))
  done
  exit 0
fi

DRIFTED=""

i=0
while [ "$i" -lt "${#PACKAGES[@]}" ]; do
  package="${PACKAGES[$i]}"
  gen_dir="${GEN_DIRS[$i]}"
  committed="$ROOT/$package/$gen_dir"
  fresh="$WORK_DIR/$package"

  mkdir -p "$fresh"
  generate_into "$package" "$fresh" "${PACKAGE_ARGS[$i]}"

  mkdir -p "$committed"
  (cd "$committed" && find . -type f | sort) >"$WORK_DIR/committed.txt"
  (cd "$fresh/$gen_dir" && find . -type f | sort) >"$WORK_DIR/fresh.txt"

  report=""
  while IFS= read -r file; do
    [ -n "$file" ] || continue
    report+="  + ${file#./}  generated from the protos, never committed"$'\n'
  done < <(comm -13 "$WORK_DIR/committed.txt" "$WORK_DIR/fresh.txt")

  while IFS= read -r file; do
    [ -n "$file" ] || continue
    report+="  - ${file#./}  committed, but no proto generates it any more"$'\n'
  done < <(comm -23 "$WORK_DIR/committed.txt" "$WORK_DIR/fresh.txt")

  while IFS= read -r file; do
    [ -n "$file" ] || continue
    cmp -s "$committed/$file" "$fresh/$gen_dir/$file" && continue
    diff -u "$committed/$file" "$fresh/$gen_dir/$file" >"$WORK_DIR/file.diff" || true
    # `grep -c` exits 1 on a zero count, which is a legitimate half of a diffstat.
    added="$(grep -cE '^\+[^+]' "$WORK_DIR/file.diff" | tr -d ' ' || true)"
    removed="$(grep -cE '^-[^-]' "$WORK_DIR/file.diff" | tr -d ' ' || true)"
    report+="  ~ ${file#./}  stale (+$added/-$removed lines)"$'\n'
    if [ "$SHOW_DIFF" = 1 ]; then
      report+="$(cat "$WORK_DIR/file.diff")"$'\n'
    fi
  done < <(comm -12 "$WORK_DIR/committed.txt" "$WORK_DIR/fresh.txt")

  if [ -n "$report" ]; then
    DRIFTED="$DRIFTED$package"$'\n'
    echo "$package/$gen_dir does not match its protos:"
    printf '%s' "$report"
  else
    echo "$package/$gen_dir is up to date."
  fi
  i=$((i + 1))
done

if [ -n "$DRIFTED" ]; then
  echo
  echo "Generated code has drifted from the protos it is generated from."
  echo "Regenerate and commit the result:"
  echo
  printf '%s' "$DRIFTED" | while IFS= read -r package; do
    echo "  scripts/generated-code.sh write $package"
  done
  exit 1
fi
