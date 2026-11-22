//! A work item's commit flow (`#workqueue` 11/14) on real repositories: a bare origin, three stacked
//! branches (feat/one ← feat/two ← feat/three on master), and a fix pushed to feat/one after the others
//! were cut. feat/three rewrote f.txt's first line; the fix rewrites it too.

use std::path::{Path, PathBuf};
use std::process::Command;

use pretty_assertions::assert_eq;
use tddy_pr_stack::commit_flow::{CommitFlow, CommitSource, ItemBranch, Prediction};
use tddy_pr_stack::lineage::LineEndpoint;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "dev")
        .env("GIT_AUTHOR_EMAIL", "dev@example.com")
        .env("GIT_COMMITTER_NAME", "dev")
        .env("GIT_COMMITTER_EMAIL", "dev@example.com")
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn commit(dir: &Path, file: &str, content: &str, subject: &str) {
    std::fs::write(dir.join(file), content).unwrap();
    git(dir, &["add", file]);
    git(dir, &["commit", "-m", subject]);
}

struct AStackedRepo {
    _root: tempfile::TempDir,
    work: PathBuf,
    root: PathBuf,
}

impl AStackedRepo {
    fn new() -> Self {
        let root_dir = tempfile::tempdir().unwrap();
        let root = root_dir.path().to_path_buf();
        let origin = root.join("origin.git");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--bare", "-b", "master"]);
        git(&root, &["clone", origin.to_str().unwrap(), "work"]);
        let work = root.join("work");
        git(&work, &["checkout", "-b", "master"]);
        commit(&work, "f.txt", "base\nsecond\n", "base");
        commit(&work, "g.txt", "g\n", "g");
        git(&work, &["push", "-u", "origin", "master"]);
        git(&work, &["checkout", "-b", "feat/one"]);
        commit(&work, "one.txt", "one\n", "one");
        git(&work, &["push", "-u", "origin", "feat/one"]);
        git(&work, &["checkout", "-b", "feat/two"]);
        commit(&work, "two.txt", "two\n", "two");
        git(&work, &["push", "-u", "origin", "feat/two"]);
        git(&work, &["checkout", "-b", "feat/three"]);
        commit(&work, "f.txt", "three\nsecond\n", "three rewrites f");
        git(&work, &["push", "-u", "origin", "feat/three"]);
        Self {
            _root: root_dir,
            work,
            root,
        }
    }

    /// A commit pushed to `branch` after the branches above it were cut.
    fn push_to(&self, branch: &str, file: &str, content: &str, subject: &str) {
        git(&self.work, &["checkout", branch]);
        commit(&self.work, file, content, subject);
        git(&self.work, &["push", "origin", branch]);
    }

    /// A fresh clone the analysis runs in (the queue owner's worktree).
    fn an_analysis_clone(&self) -> PathBuf {
        git(
            &self.root,
            &[
                "clone",
                self.root.join("origin.git").to_str().unwrap(),
                "analysis",
            ],
        );
        self.root.join("analysis")
    }
}

fn the_queue() -> Vec<ItemBranch> {
    vec![
        ItemBranch {
            item_id: "n1".into(),
            branch: "feat/one".into(),
            line_parent: LineEndpoint::Trunk("master".into()),
        },
        ItemBranch {
            item_id: "n2".into(),
            branch: "feat/two".into(),
            line_parent: LineEndpoint::Item("n1".into()),
        },
        ItemBranch {
            item_id: "n3".into(),
            branch: "feat/three".into(),
            line_parent: LineEndpoint::Item("n2".into()),
        },
    ]
}

fn subjects_and_sources(flow: &CommitFlow, item: &str) -> Vec<(String, CommitSource)> {
    flow.inbox(&the_queue(), item)
        .expect("inbox")
        .into_iter()
        .map(|e| (e.commit.subject, e.source))
        .collect()
}

#[test]
fn a_commit_pushed_to_an_ancestor_after_the_cut_is_in_the_items_inbox_attributed_to_it() {
    // Given
    let repo = AStackedRepo::new();
    repo.push_to("feat/one", "f.txt", "fix\nsecond\n", "fix one");
    let flow = CommitFlow::new(repo.an_analysis_clone());
    flow.fetch().expect("fetch");

    // Then — reaching n2 and n3 alike, attributed to n1
    for item in ["n2", "n3"] {
        assert_eq!(
            subjects_and_sources(&flow, item),
            vec![("fix one".to_string(), CommitSource::Item("n1".to_string()))],
            "{item}"
        );
    }
}

#[test]
fn an_items_outbox_equals_its_successors_inbox_entries_from_it() {
    // Given
    let repo = AStackedRepo::new();
    repo.push_to("feat/one", "f.txt", "fix\nsecond\n", "fix one");
    let flow = CommitFlow::new(repo.an_analysis_clone());
    flow.fetch().expect("fetch");

    // When
    let outbox: Vec<(String, String)> = flow
        .outbox(&the_queue(), "n1")
        .expect("outbox")
        .into_iter()
        .map(|e| (e.successor, e.commit.subject))
        .collect();

    // Then
    assert_eq!(outbox, vec![("n2".to_string(), "fix one".to_string())]);
}

#[test]
fn one_commit_can_be_clean_for_one_item_and_conflicting_for_the_next() {
    // Given
    let repo = AStackedRepo::new();
    repo.push_to("feat/one", "f.txt", "fix\nsecond\n", "fix one");
    let flow = CommitFlow::new(repo.an_analysis_clone());
    flow.fetch().expect("fetch");

    // When
    let at_n2 = flow.inbox(&the_queue(), "n2").expect("inbox n2");
    let at_n3 = flow.inbox(&the_queue(), "n3").expect("inbox n3");

    // Then — and the conflicting prediction names the file
    assert_eq!(
        (at_n2[0].prediction.clone(), at_n3[0].prediction.clone()),
        (
            Prediction::Clean,
            Prediction::Conflicts(vec!["f.txt".to_string()])
        )
    );
}

#[test]
fn trunk_commits_are_an_external_source_listed_first() {
    // Given
    let repo = AStackedRepo::new();
    repo.push_to("feat/one", "one.txt", "one\nmore\n", "more one");
    repo.push_to("master", "g.txt", "g\ntrunk\n", "trunk moved");
    let flow = CommitFlow::new(repo.an_analysis_clone());
    flow.fetch().expect("fetch");

    // Then
    assert_eq!(
        subjects_and_sources(&flow, "n2"),
        vec![
            (
                "trunk moved".to_string(),
                CommitSource::Trunk("master".to_string())
            ),
            ("more one".to_string(), CommitSource::Item("n1".to_string())),
        ]
    );
}

#[test]
fn the_analysis_changes_no_local_branch_index_or_worktree() {
    // Given
    let repo = AStackedRepo::new();
    repo.push_to("feat/one", "f.txt", "fix\nsecond\n", "fix one");
    let clone = repo.an_analysis_clone();
    std::fs::write(clone.join("scratch.txt"), "the owner's uncommitted work\n").unwrap();
    let before = (
        git(&clone, &["for-each-ref", "refs/heads"]),
        git(&clone, &["rev-parse", "HEAD"]),
        git(&clone, &["status", "--porcelain"]),
    );
    let flow = CommitFlow::new(&clone);

    // When
    flow.fetch().expect("fetch");
    for item in ["n1", "n2", "n3"] {
        flow.inbox(&the_queue(), item).expect("inbox");
        flow.outbox(&the_queue(), item).expect("outbox");
    }

    // Then
    assert_eq!(
        (
            git(&clone, &["for-each-ref", "refs/heads"]),
            git(&clone, &["rev-parse", "HEAD"]),
            git(&clone, &["status", "--porcelain"]),
        ),
        before
    );
}

#[test]
fn unchanged_tips_answer_from_the_cache() {
    // Given
    let repo = AStackedRepo::new();
    repo.push_to("feat/one", "f.txt", "fix\nsecond\n", "fix one");
    let flow = CommitFlow::new(repo.an_analysis_clone());
    flow.fetch().expect("fetch");

    // When
    let first = flow.inbox(&the_queue(), "n2").expect("first");
    let second = flow.inbox(&the_queue(), "n2").expect("second");

    // Then
    assert_eq!((first, flow.cache_hits()), (second, 1));
}

#[test]
fn an_items_own_commits_are_those_beyond_its_line_parent() {
    // Given
    let repo = AStackedRepo::new();
    let flow = CommitFlow::new(repo.an_analysis_clone());
    flow.fetch().expect("fetch");

    // When
    let own: Vec<String> = flow
        .own_commits(&the_queue(), "n3")
        .expect("own")
        .into_iter()
        .map(|c| c.subject)
        .collect();

    // Then
    assert_eq!(own, vec!["three rewrites f".to_string()]);
}
