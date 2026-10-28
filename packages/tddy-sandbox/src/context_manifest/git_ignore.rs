//! Asking git which pulled-in paths it ignores, failing closed when it cannot answer.

use std::collections::BTreeSet;
use std::path::Path;

/// Which of `rel_paths` (relative to `root`, canonical) git ignores, asked in **one**
/// `git check-ignore --stdin -z` per call rather than one process per file.
///
/// `check-ignore` without `--no-index` is the question wanted: it applies the ignore rules
/// (`.gitignore` at every level, `.git/info/exclude`, `core.excludesFile`) to untracked paths, and
/// answers *not ignored* for a tracked file even where a pattern matches it — a committed file is
/// published already. A path inside an ignored directory is reported ignored.
///
/// **Fails closed.** When `root` is not in a git repository, or git cannot be run or answers with
/// anything but its two "answered" exit codes (`0` some ignored, `1` none), every path asked about
/// is treated as ignored and the reason logged at `warn`. That is a narrowing, not a fallback: a
/// link then brings nothing a glob does not also name, and the agent reads less of what the project
/// ships rather than something it should not have.
pub(super) fn git_ignored<'a>(
    root: &Path,
    rel_paths: impl Iterator<Item = &'a str>,
) -> BTreeSet<String> {
    let asked: BTreeSet<String> = rel_paths.map(str::to_string).collect();
    if asked.is_empty() {
        return asked;
    }
    match ask_git_check_ignore(root, &asked) {
        Ok(ignored) => ignored,
        Err(e) => {
            log::warn!(
                "context_manifest: cannot tell which of {} link-reached path(s) under {root:?} git \
                 ignores ({e:#}); pulling none of them in",
                asked.len()
            );
            asked
        }
    }
}

fn ask_git_check_ignore(root: &Path, asked: &BTreeSet<String>) -> anyhow::Result<BTreeSet<String>> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    // Answered by `root`'s own repository and nothing else: an inherited `GIT_DIR` or
    // `GIT_WORK_TREE` would point the query at another one, and without a ceiling a `root` that is not
    // itself a repository would be answered by whatever repository encloses it instead of failing.
    let mut command = Command::new("git");
    for variable in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
    ] {
        command.env_remove(variable);
    }
    if let Some(parent) = root.parent() {
        command.env("GIT_CEILING_DIRECTORIES", parent);
    }
    let mut child = command
        .arg("-C")
        .arg(root)
        .args(["check-ignore", "--stdin", "-z"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("running git check-ignore failed: {e}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("git check-ignore has no stdin"))?;
    // Every path is asked as `./<path>`. Bare, one starting with `:` is read as pathspec magic —
    // `:secrets` is evaluated as `secrets`, so an ignored file of that name would come back as not
    // ignored and be pulled in. Git echoes the prefix, and it is stripped from the answer below.
    let input: Vec<u8> = asked
        .iter()
        .flat_map(|path| {
            LITERAL_PATH_PREFIX
                .bytes()
                .chain(path.bytes())
                .chain(std::iter::once(0))
        })
        .collect();
    // Fed from a thread: git answers as it reads, and a list long enough to fill both pipes would
    // otherwise deadlock the two processes on each other.
    let feeder = std::thread::spawn(move || stdin.write_all(&input));
    let output = child.wait_with_output()?;
    let fed = feeder
        .join()
        .map_err(|_| anyhow::anyhow!("the thread feeding git check-ignore panicked"))?;
    // The exit status first: a git that refused (`not a git repository`) may have stopped reading
    // its stdin, and the broken pipe that follows is not the reason worth reporting.
    if !matches!(output.status.code(), Some(0) | Some(1)) {
        anyhow::bail!(
            "git check-ignore exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    fed.map_err(|e| anyhow::anyhow!("feeding paths to git check-ignore failed: {e}"))?;
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            let path = String::from_utf8_lossy(path);
            path.strip_prefix(LITERAL_PATH_PREFIX)
                .unwrap_or(&path)
                .to_string()
        })
        .collect())
}

/// What every path asked of `git check-ignore` is spelled with, so none is read as pathspec magic.
const LITERAL_PATH_PREFIX: &str = "./";
