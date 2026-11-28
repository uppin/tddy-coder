#!/usr/bin/env bash
# Runs the Cursor agent CLI (`agent -p`, on Composer or glm-5p3-flash) on this machine (host `local`)
# and lets a driver follow, judge and resume its turns.
# Usage: cursor-agent.sh <command> [args]   (see `cursor-agent.sh help`)
#
# Output contract for agents: one KEY=VALUE line per fact on stdout, diagnostics on stderr, and a
# distinct exit code for each failure the caller must act on.
set -euo pipefail

EXIT_USAGE=2
EXIT_NOT_LOGGED_IN=10   # the Cursor agent is not logged in — the user must log in
EXIT_NO_MODEL=12        # the account has no Composer or GLM model
EXIT_BUSY=14            # a turn of that name is still running, or another agent holds the worktree
EXIT_RUN_LOST=16        # RUN_EXIT of a turn killed by a machine stop or reboot — resume its session
EXIT_NO_CLI=19          # the Cursor agent CLI is not installed — the user installs it
EXIT_NO_TOOL=20         # jq is missing — the user installs it

RUNS_DIR="$HOME/.composer-runs"   # <name>/stream.jsonl, prompt-<n>.txt, run-<n>.sh, exit-<n>, pid

die() { local code="$1"; shift; echo "cursor-agent: $*" >&2; exit "$code"; }
q() { printf '%q' "$1"; }

# Only this machine is a host. Anything else is a usage error.
need_local() { [[ "$1" == local ]] || die $EXIT_USAGE "host must be 'local' (only this machine is supported): '$1'"; }

valid_name() { [[ "$1" =~ ^[A-Za-z0-9._-]+$ ]] || die $EXIT_USAGE "name must match [A-Za-z0-9._-]+: '$1'"; }

watch_cmd() { echo "tail -f $RUNS_DIR/$1/stream.jsonl"; }

# The tools a turn needs, the Cursor agent and its login, and the models. This machine is the
# developer's, so the developer installs and logs in — never this script.
cmd_ensure() {
  local host="${1:?host: local}" out models composer glm
  need_local "$host"
  strip() { sed $'s/\x1b\\[[0-9;?]*[a-zA-Z]//g'; }

  command -v jq >/dev/null 2>&1 || {
    [[ "$(uname -s)" == Darwin ]] && die $EXIT_NO_TOOL "jq is missing. The user installs it: brew install jq"
    die $EXIT_NO_TOOL "jq is missing. The user installs it with the system package manager"
  }
  command -v agent >/dev/null 2>&1 \
    || die $EXIT_NO_CLI "the Cursor agent CLI is not installed. The user runs: curl https://cursor.com/install -fsS | bash"
  echo "CURSOR_AGENT_VERSION=$(agent --version 2>/dev/null | strip | tail -1)"

  out="$(agent status 2>/dev/null | strip || true)"
  grep -q "Logged in" <<<"$out" || die $EXIT_NOT_LOGGED_IN "the Cursor agent is not logged in. The user runs: agent login"
  echo "CURSOR_LOGGED_IN=yes"

  # --list-models needs a logged-in agent.
  models="$(agent --list-models 2>/dev/null | strip || true)"
  composer="$(grep -oE '^composer-[0-9][0-9.]*' <<<"$models" | sort -V | tail -1 || true)"
  # glm-5p3-flash is unlisted: --list-models never shows it, so probe it with a one-line turn instead.
  glm=""
  if agent -p --model glm-5p3-flash --output-format text "Reply with exactly: ok" 2>/dev/null | grep -q ok; then
    glm="glm-5p3-flash"
  fi
  echo "COMPOSER_MODEL=$composer"
  echo "GLM_MODEL=$glm"
  [[ -n "$composer" || -n "$glm" ]] || die $EXIT_NO_MODEL "no composer model in 'agent --list-models', and glm-5p3-flash did not answer a probe"
  echo "READY=yes"
}

cmd_exec() {
  local host="${1:?host}" dir="${2:?dir}"; shift 2
  need_local "$host"
  [[ $# -gt 0 ]] || die $EXIT_USAGE "exec needs a command"
  bash -c "cd $(q "$dir") && $*"
}

# ── Turns ─────────────────────────────────────────────────────────────────────────────────────────
# A unit of work is a <name> (one Cursor session). Everything about it lives in
# ~/.composer-runs/<name>/: the stream every turn appends to, the prompts, and each turn's exit
# status. Only a machine stop or reboot kills a turn, and the turn is then continued with --resume.
# A turn is a background process in its own process group (so `cancel` takes the agent's children
# with it), writing the stream itself.

turns_of() { ls "$1" 2>/dev/null | grep -c '^prompt-' || true; }

is_running() {
  local rd="$1" n
  n="$(turns_of "$rd")"
  [[ $n -gt 0 && ! -f "$rd/exit-$n" && -f "$rd/pid" ]] && kill -0 "$(cat "$rd/pid")" 2>/dev/null
}

# Creates <file> holding <pid> unless a live process already holds it. noclobber makes the create
# atomic, so of two runs racing for one name or one worktree exactly one gets past here.
reserve() {
  local file="$1" pid="$2"
  ( set -C; echo "$pid" >"$file" ) 2>/dev/null && return 0
  kill -0 "$(cat "$file" 2>/dev/null)" 2>/dev/null && return 1
  rm -f "$file"
  ( set -C; echo "$pid" >"$file" ) 2>/dev/null
}

cmd_run() {
  local host="" name="" dir="" prompt_file="" resume="" model=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --host) host="$2"; shift 2 ;;
      --name) name="$2"; shift 2 ;;
      --dir) dir="$2"; shift 2 ;;
      --prompt-file) prompt_file="$2"; shift 2 ;;
      --resume) resume="$2"; shift 2 ;;
      --model) model="$2"; shift 2 ;;
      *) die $EXIT_USAGE "run: unknown argument $1" ;;
    esac
  done
  [[ -n "$host" && -n "$name" && -n "$dir" && -n "$prompt_file" && -n "$model" ]] \
    || die $EXIT_USAGE "run needs --host --name --dir --prompt-file --model [--resume <session_id>]"
  need_local "$host"
  valid_name "$name"
  [[ -f "$prompt_file" ]] || die $EXIT_USAGE "no prompt file $prompt_file"

  local rd="$RUNS_DIR/$name" n lock
  mkdir -p "$rd"
  reserve "$rd/launching" $$ || die $EXIT_BUSY "a turn of $name is starting"
  trap "rm -f $(q "$rd/launching")" EXIT
  is_running "$rd" && die $EXIT_BUSY "a turn of $name is still running"
  # A pid file, not flock (macOS has none): a second agent in the same worktree exits 14. It holds
  # this launcher's pid until the turn's own pid replaces it below.
  lock="$(git -C "$dir" rev-parse --absolute-git-dir)/cursor-agent.pid"
  reserve "$lock" $$ || die $EXIT_BUSY "another agent holds the worktree $dir (pid $(cat "$lock"))"
  n=$(( $(turns_of "$rd") + 1 ))
  cp "$prompt_file" "$rd/prompt-$n.txt"
  {
    printf 'CA_DIR=%q CA_LOCK=%q CA_TURN=%q CA_RESUME=%q CA_MODEL=%q CA_PROMPT=%q CA_EXIT=%q\n' \
      "$dir" "$lock" "$n" "$resume" "$model" "$rd/prompt-$n.txt" "$rd/exit-$n"
    cat <<'RUN'
cd "$CA_DIR" || exit 1
printf '{"type":"composer-agent","event":"turn-start","turn":%s,"at":"%s","resume":"%s"}\n' "$CA_TURN" "$(date -u +%FT%TZ)" "$CA_RESUME"
agent -p --output-format stream-json --trust --force --model "$CA_MODEL" --workspace "$CA_DIR" \
  ${CA_RESUME:+--resume "$CA_RESUME"} "$(cat "$CA_PROMPT")" </dev/null
rc=$?
rm -f "$CA_LOCK"
printf '\n{"type":"composer-agent","event":"turn-end","turn":%s,"at":"%s","exit":%d}\n' "$CA_TURN" "$(date -u +%FT%TZ)" $rc
echo $rc >"$CA_EXIT"
RUN
  } >"$rd/run-$n.sh"
  # set -m gives the turn its own process group; nohup lets it outlive the session that started it.
  bash -c 'set -m; nohup bash "$0" >>"$1" 2>&1 </dev/null & echo $! >"$2"; echo $! >"$3"' \
    "$rd/run-$n.sh" "$rd/stream.jsonl" "$rd/pid" "$lock"
  echo "TURN=$n"
  echo "NAME=$name"
  echo "WATCH=$(watch_cmd "$name")"
}

# Blocks until the name's current turn has ended, or `secs` pass. The default stays under the agent
# harness's ~3-minute foreground limit: a longer call is moved to the background, which ends the
# driver's turn mid-stage. A long turn is followed by calling this repeatedly. A turn whose process
# is gone without a turn-end marker was killed by a machine stop: it is marked ended with exit 16.
cmd_wait() {
  local host="${1:?host}" name="${2:?name}" secs="${3:-170}" rd n waited=0
  need_local "$host"; valid_name "$name"; rd="$RUNS_DIR/$name"
  [[ -d "$rd" ]] || die $EXIT_USAGE "no turns for $name"
  n="$(turns_of "$rd")"
  while is_running "$rd"; do
    [[ $waited -ge $secs ]] && { echo WAIT=still-running; echo "TURN=$n"; return 0; }
    sleep 5; waited=$((waited + 5))
  done
  if [[ ! -f "$rd/exit-$n" ]]; then
    printf '\n{"type":"composer-agent","event":"turn-end","turn":%s,"at":"%s","exit":%s}\n' "$n" "$(date -u +%FT%TZ)" $EXIT_RUN_LOST >>"$rd/stream.jsonl"
    echo $EXIT_RUN_LOST >"$rd/exit-$n"
  fi
  echo WAIT=turn-ended; echo "TURN=$n"; echo "RUN_EXIT=$(cat "$rd/exit-$n")"
}

# Shared jq: the stream's lines, with stray CRs and terminal escapes dropped, and $last = the events
# of the current (last) turn.
JQ_EVENTS='[split("\n")[] | sub("\r$"; "") | sub("^[^{]*"; "") | (try fromjson catch null) | objects] as $all
  | ($all | map(.type == "composer-agent" and .event == "turn-start") | rindex(true) // -1) as $i
  | ($all[($i + 1):]) as $last'

stream_of() {
  local name="$1" f="$RUNS_DIR/$1/stream.jsonl"
  [[ -f "$f" ]] || die $EXIT_USAGE "no stream for $name"
  echo "$f"
}

# The session to resume and how the LAST turn ended.
cmd_summary() {
  local host="${1:?host}" name="${2:?name}" f
  need_local "$host"; valid_name "$name"; f="$(stream_of "$name")"
  jq -rRs "$JQ_EVENTS"'
| ($all | map(.session_id // empty) | last) as $sid
| ($last | map(select(.type == "result")) | last) as $res
| ($last | map(select(.type == "composer-agent" and .event == "turn-end")) | last) as $fin
| (($res.result // "") | if test("\\S") then . else ($last | map(select(.type == "assistant")) | last | .message.content[0].text? // "") end) as $shown
| "SESSION_ID=\($sid // "")",
  "TURNS=\($all | map(select(.type == "composer-agent" and .event == "turn-start")) | length)",
  "LAST_TURN_STATE=\(if $res then (if $res.is_error then "error" else "finished" end)
                    elif $fin then "interrupted" else "running" end)",
  "LAST_TURN_EXIT=\($fin.exit // "")",
  "LAST_TURN_TOOL_CALLS=\($last | map(select(.type == "tool_call" and .subtype == "started")) | length)",
  "RESULT_EMPTY=\(if ($shown | test("\\S")) then "no" else "yes" end)",
  "RESULT<<EOF",
  $shown,
  "EOF",
  "LAST_THOUGHT<<EOF",
  ($last | map(select(.type == "thinking" and .subtype == "delta") | .text) | join("") | .[-600:]),
  "EOF"' "$f"
}

# The last n tool calls and messages of the current turn — what the agent is doing, minus tokens.
cmd_progress() {
  local host="${1:?host}" name="${2:?name}" n="${3:-15}" f
  need_local "$host"; valid_name "$name"; f="$(stream_of "$name")"
  jq -rRs --argjson n "$n" "$JQ_EVENTS"'
| $last
| map(
    if .type == "tool_call" and .subtype == "started" then
      (.tool_call | to_entries[] | select(.value | type == "object" and has("args"))) as $t
      | "tool  \($t.key | sub("ToolCall$"; ""))  \($t.value.args.command // $t.value.args.path // $t.value.description // "" | tostring | .[0:160])"
    elif .type == "assistant" then "say   \(.message.content[0].text? // "" | gsub("\n"; " ") | .[0:160])"
    elif .type == "result" then "end   \(.subtype)"
    elif .type == "composer-agent" and .event == "turn-end" then "exit  \(.exit)"
    else empty end)
| .[-$n:][]' "$f"
}

# Stops the name's current turn: kills its process group, so the agent and its lock go with it.
cmd_cancel() {
  local host="${1:?host}" name="${2:?name}" rd n
  need_local "$host"; valid_name "$name"; rd="$RUNS_DIR/$name"
  n="$(turns_of "$rd")"
  is_running "$rd" && kill -TERM -- "-$(cat "$rd/pid")" 2>/dev/null || true
  if [[ $n -gt 0 && ! -f "$rd/exit-$n" ]]; then
    printf '\n{"type":"composer-agent","event":"turn-end","turn":%s,"exit":130}\n' "$n" >>"$rd/stream.jsonl"
    echo 130 >"$rd/exit-$n"
  fi
  echo "CANCELLED=$name"
}

cmd_help() {
  cat <<'EOF'
usage — <host> is always `local` (this machine):
  cursor-agent.sh ensure local                    jq, the Cursor agent CLI and its login; prints COMPOSER_MODEL and GLM_MODEL
  cursor-agent.sh exec local <dir> <cmd…>         run a shell command in a dir
  cursor-agent.sh run --host local --name <name> --dir <dir> --prompt-file <file> --model <m> [--resume <session_id>]
                                                    start a turn (detached process) and return;
                                                    the stream is ~/.composer-runs/<name>/stream.jsonl
  cursor-agent.sh wait local <name> [secs=170]    block until the turn ends: WAIT=turn-ended (+RUN_EXIT) | still-running
  cursor-agent.sh progress local <name> [n=15]    last n tool calls and messages of the current turn
  cursor-agent.sh summary local <name>            SESSION_ID, LAST_TURN_STATE and RESULT of the last turn
  cursor-agent.sh cancel local <name>             kill the current turn
exit codes: 2 usage (incl. a host other than local) · 10 Cursor not logged in · 12 no Composer or GLM model · 14 busy · 19 install the Cursor agent CLI · 20 install jq
RUN_EXIT 16 = the turn was killed by a machine stop or reboot (resume its session)
EOF
}

sub="${1:-help}"; shift || true
case "$sub" in
  ensure) cmd_ensure "$@" ;;
  exec) cmd_exec "$@" ;;
  run) cmd_run "$@" ;;
  wait) cmd_wait "$@" ;;
  progress) cmd_progress "$@" ;;
  summary) cmd_summary "$@" ;;
  cancel) cmd_cancel "$@" ;;
  help|-h|--help) cmd_help ;;
  *) cmd_help >&2; exit $EXIT_USAGE ;;
esac
