#!/usr/bin/env bash
# Resolve this host's build cache (sccache) settings and print them as `export`
# lines for the caller to eval. Nothing is exported by this script itself — it
# runs in a subshell.
#
#   eval "$(scripts/build-cache-env.sh)"
#
# Where the settings come from, in precedence order:
#
#   1. TDDY_BUILD_CACHE — names the backend directly and the YAML is never read.
#      This is how CI is wired: .github/workflows/ci.yml sets it to
#      `github-actions`, so a runner needs no per-host file.
#   2. ~/.tddy/build-cache.yaml (override the path with TDDY_BUILD_CACHE_CONFIG)
#      — the per-host file, deliberately outside the repo so it is never
#      committed and so every worktree of this repo shares one answer.
#   3. Neither — sccache is left off entirely. No RUSTC_WRAPPER is emitted and
#      nothing is written to the user's disk. There is no implicit local cache:
#      a build cache is opt-in, per host.
#
# Backends and the coordinates each needs:
#
#   local           dir (default ~/.cache/sccache), max_size (default 10G)
#   redis           url (required), key_prefix
#   github-actions  url + token (default to the runner's ACTIONS_RESULTS_URL /
#                   ACTIONS_RUNTIME_TOKEN), version
#   off             explicitly no cache, and no notice about it
#
# See docs/dev/guides/build-cache.md.
set -euo pipefail

QUIET=0
if [ "${1:-}" = "--quiet" ]; then
  QUIET=1
fi

CONFIG_PATH="${TDDY_BUILD_CACHE_CONFIG:-$HOME/.tddy/build-cache.yaml}"
DOC="docs/dev/guides/build-cache.md"

note() {
  [ "$QUIET" -eq 1 ] || printf 'build-cache: %s\n' "$1" >&2
}

fail() {
  printf 'build-cache: %s\n' "$1" >&2
  exit 1
}

# Flatten the fixed two-level `sccache:` schema into `path<TAB>value` lines:
# `type`, and `<backend>.<key>` for each backend block. Everything outside the
# `sccache:` mapping is ignored, so the file can grow other top-level sections
# without this parser having to learn them.
#
# A real YAML parser is not available here: this runs before the nix shell, on
# whatever bash and awk the host ships (bash 3.2 on macOS — no associative
# arrays). The schema is small and fixed, so a strict reader for exactly that
# shape is the honest trade; anything it does not recognise is dropped rather
# than guessed at.
read -r -d '' FLATTEN_YAML <<'AWK_EOF' || true
{
  line = $0
  sub(/\r$/, "", line)
  sub(/[[:space:]]+#.*$/, "", line)
  if (line ~ /^[[:space:]]*#/) next
  if (line ~ /^[[:space:]]*$/) next

  match(line, /^[[:space:]]*/)
  indent = RLENGTH

  entry = line
  sub(/^[[:space:]]*/, "", entry)
  colon = index(entry, ":")
  if (colon == 0) next

  key = substr(entry, 1, colon - 1)
  val = substr(entry, colon + 1)
  sub(/^[[:space:]]+/, "", val)
  sub(/[[:space:]]+$/, "", val)
  gsub(/^["']|["']$/, "", val)

  if (indent == 0) { top = key; section = ""; next }
  if (top != "sccache") next

  if (indent == 2) {
    if (val == "") { section = key; next }
    section = ""
    print key "\t" val
    next
  }

  if (indent == 4 && section != "" && val != "") {
    print section "." key "\t" val
  }
}
AWK_EOF

PARSED=""

# Look up one flattened path; prints nothing when it is absent.
cfg() {
  printf '%s\n' "$PARSED" | awk -F'\t' -v want="$1" '$1 == want { print $2; exit }'
}

# Only a leading `~/` is expanded — the shapes that appear here are paths a
# person typed, not shell words.
expand_home() {
  case "$1" in
    "~/"*) printf '%s' "$HOME/${1#\~/}" ;;
    "~")   printf '%s' "$HOME" ;;
    *)     printf '%s' "$1" ;;
  esac
}

emit() {
  printf 'export %s=%s\n' "$1" "$(quote "$2")"
}

quote() {
  printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"
}

if [ -n "${TDDY_BUILD_CACHE:-}" ]; then
  KIND="$TDDY_BUILD_CACHE"
  SOURCE="TDDY_BUILD_CACHE"
elif [ -e "$CONFIG_PATH" ]; then
  [ -r "$CONFIG_PATH" ] || fail "$CONFIG_PATH is not readable"
  PARSED="$(awk "$FLATTEN_YAML" "$CONFIG_PATH")"
  KIND="$(cfg type)"
  SOURCE="$CONFIG_PATH"
  [ -n "$KIND" ] || fail "$CONFIG_PATH has no 'sccache.type' — see $DOC"
else
  note "no $CONFIG_PATH — sccache off"
  note "             (see $DOC)"
  exit 0
fi

case "$KIND" in
  off|none|disabled)
    exit 0
    ;;

  local)
    DIR="$(expand_home "$(cfg local.dir)")"
    [ -n "$DIR" ] || DIR="$HOME/.cache/sccache"
    MAX_SIZE="$(cfg local.max_size)"
    [ -n "$MAX_SIZE" ] || MAX_SIZE="10G"
    emit SCCACHE_DIR "$DIR"
    emit SCCACHE_CACHE_SIZE "$MAX_SIZE"
    note "sccache → local $DIR (max $MAX_SIZE)"
    ;;

  redis)
    URL="$(cfg redis.url)"
    [ -n "$URL" ] || fail "sccache.type is 'redis' but no 'redis.url' in $SOURCE — see $DOC"
    emit SCCACHE_REDIS "$URL"
    PREFIX="$(cfg redis.key_prefix)"
    if [ -n "$PREFIX" ]; then
      emit SCCACHE_REDIS_KEY_PREFIX "$PREFIX"
    fi
    # Redacted: the URL carries the password when the server needs one.
    _rest="${URL#*://}"
    note "sccache → redis ${URL%%://*}://${_rest#*@}"
    ;;

  github-actions|gha)
    URL="$(cfg github-actions.url)"
    [ -n "$URL" ] || URL="${ACTIONS_RESULTS_URL:-}"
    TOKEN="$(cfg github-actions.token)"
    [ -n "$TOKEN" ] || TOKEN="${ACTIONS_RUNTIME_TOKEN:-}"
    # A runner exposes these to a step only when something asks for them, and an
    # sccache that cannot reach the cache service silently compiles everything
    # twice instead. Refuse rather than pretend to be caching.
    [ -n "$URL" ] || fail "sccache.type is 'github-actions' but neither ACTIONS_RESULTS_URL nor github-actions.url is set — see $DOC"
    [ -n "$TOKEN" ] || fail "sccache.type is 'github-actions' but neither ACTIONS_RUNTIME_TOKEN nor github-actions.token is set — see $DOC"
    emit SCCACHE_GHA_ENABLED "true"
    emit ACTIONS_RESULTS_URL "$URL"
    emit ACTIONS_RUNTIME_TOKEN "$TOKEN"
    VERSION="$(cfg github-actions.version)"
    if [ -n "$VERSION" ]; then
      emit SCCACHE_GHA_VERSION "$VERSION"
    fi
    note "sccache → github-actions cache"
    ;;

  *)
    fail "unknown sccache.type '$KIND' in $SOURCE — expected local, redis, github-actions or off"
    ;;
esac

# Common to every enabled backend.
#
# CARGO_INCREMENTAL=0 is not a preference. sccache refuses to cache a
# compilation carrying `-C incremental`, so leaving cargo's dev-profile default
# on would hand it a stream of units it declines and the cache would stay empty.
emit RUSTC_WRAPPER "sccache"
emit CARGO_INCREMENTAL "0"
