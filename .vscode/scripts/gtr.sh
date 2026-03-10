#!/bin/zsh
# gtr ─ Git worktree helper
#
# Examples
# --------
#   gtr create feature0          # add ~/code/worktrees/feature0 (branch claude/feature0)
#   gtr create feat1 feat2       # add two worktrees at once
#   gtr rm feature0              # remove the worktree directory
#   gtr cd feature0              # jump into the worktree directory
#   gtr claude feature0          # run `claude` while inside that worktree
#
# ------------------------------------------------------------

gtr () {
  local cmd="$1"; shift || { echo "Usage: gtr {create|rm|cd|claude} <name>"; return 1; }

  # Base folder for all worktrees
  local base="$HOME/code/worktrees"

  case "$cmd" in
    create)
      for name in "$@"; do
        git worktree add "$base/$name" -b "claude/$name"
      done
      ;;

    rm|remove)
      for name in "$@"; do
        git worktree remove "$base/$name"
      done
      ;;

    cd)
      [ -n "$1" ] || { echo "Usage: gtr cd <name>"; return 1; }
      cd "$base/$1" || { echo "No such worktree: $base/$1"; return 1; }
      ;;

    claude)
      [ -n "$1" ] || { echo "Usage: gtr claude <name>"; return 1; }
      local dir="$base/$1"

      if [ ! -d "$dir" ]; then
        # Prompt in a shell-agnostic way
        printf "Worktree '%s' doesn't exist. Create it now? [y/N] " "$1"
        read -r reply
        case "$reply" in
          [yY]|[yY][eE][sS])
            echo "Creating worktree '$1'…"
            git worktree add "$dir" -b "claude/$1" || { echo "git worktree add failed"; return 1; }
            ;;
          *)
            echo "Aborted."; return 1;;
        esac
      fi

      ( cd "$dir" && claude )
      ;;

    *)
      echo "Unknown sub-command: $cmd (use create, rm, cd, claude)"; return 1;;
  esac
}

# Call the function with all script arguments
gtr "$@"
