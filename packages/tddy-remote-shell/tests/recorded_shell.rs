//! `RecordedShell` reports every call it makes: the target, the caller's command, how long it took
//! and how it ended (`#coder-start` 8/10).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_remote_shell::{
    ExecOutput, ExecRequest, InMemoryShell, RecordedShell, RemoteCallOutcome, RemoteCallReport,
    RemoteCallSink, RemoteShell,
};

/// A sink that keeps what it is told.
#[derive(Default)]
struct ASink {
    calls: Mutex<Vec<RemoteCallReport>>,
}

impl RemoteCallSink for ASink {
    fn report(&self, call: RemoteCallReport) {
        self.calls.lock().unwrap().push(call);
    }
}

impl ASink {
    fn reported(&self) -> Vec<RemoteCallReport> {
        self.calls.lock().unwrap().clone()
    }
}

/// `target` seen through a recording shell, with the sink it reports to.
fn recorded(target: &InMemoryShell) -> (RecordedShell, Arc<ASink>) {
    let sink = Arc::new(ASink::default());
    let shell = RecordedShell::new(Arc::new(target.clone()), sink.clone());
    (shell, sink)
}

/// What a report must say, without its duration.
fn said(report: &RemoteCallReport) -> (String, String, RemoteCallOutcome, String) {
    (
        report.address.to_string(),
        report.command.clone(),
        report.outcome.clone(),
        report.stderr_tail.clone(),
    )
}

#[tokio::test]
async fn a_recorded_shell_reports_each_call_with_its_outcome() {
    // Given a target that answers one command, fails another, hangs on a third
    let target = InMemoryShell::new("remote-dev");
    target
        .answers("true", ExecOutput::succeeded(""))
        .answers(
            "git fetch --all",
            ExecOutput::exited(128, "fatal: could not read from remote repository\n"),
        )
        .hangs_on("sleep 3600");
    let (shell, sink) = recorded(&target);

    // When the three are run through the recording shell
    let _ = shell.exec(ExecRequest::new("true")).await;
    let fetched = shell.exec(ExecRequest::new("git fetch --all")).await;
    let _ = shell
        .exec(ExecRequest::new("sleep 3600").within(Duration::from_secs(5)))
        .await;

    // Then each call is reported in order, with how it ended, and the calls themselves are unchanged
    assert_eq!(
        fetched.map(|output| output.status),
        Ok(128),
        "a recorded call returns what the shell returned"
    );
    let on = "in-memory target remote-dev".to_string();
    assert_eq!(
        sink.reported().iter().map(said).collect::<Vec<_>>(),
        vec![
            (
                on.clone(),
                "true".to_string(),
                RemoteCallOutcome::Exited { code: 0 },
                String::new()
            ),
            (
                on.clone(),
                "git fetch --all".to_string(),
                RemoteCallOutcome::Exited { code: 128 },
                "fatal: could not read from remote repository".to_string()
            ),
            (
                on,
                "sleep 3600".to_string(),
                RemoteCallOutcome::TimedOut,
                String::new()
            ),
        ]
    );
}

#[tokio::test]
async fn a_call_to_an_unreachable_target_is_reported_unreachable_with_what_the_transport_said() {
    // Given a target nothing reaches
    let target = InMemoryShell::new("remote-dev");
    target.is_unreachable("Connection refused");
    let (shell, sink) = recorded(&target);

    // When a command is run through the recording shell
    let _ = shell.exec(ExecRequest::new("true")).await;

    // Then the call is reported unreachable, the stderr tail carrying the transport's words
    assert_eq!(
        sink.reported().iter().map(said).collect::<Vec<_>>(),
        vec![(
            "in-memory target remote-dev".to_string(),
            "true".to_string(),
            RemoteCallOutcome::Unreachable,
            "Connection refused".to_string()
        )]
    );
}

#[tokio::test]
async fn a_report_never_carries_a_credential() {
    // Given a command that clones from a URL with a password in it, and a target that echoes it
    let target = InMemoryShell::new("remote-dev");
    let clone = "git clone https://bot:s3cret-token@example.com/acme/repo.git /srv/repo";
    target.answers(
        clone,
        ExecOutput::exited(
            128,
            "fatal: unable to access 'https://bot:s3cret-token@example.com/acme/repo.git/'\n",
        ),
    );
    let (shell, sink) = recorded(&target);

    // When it is run through the recording shell
    let _ = shell.exec(ExecRequest::new(clone)).await;

    // Then neither the reported command nor its stderr tail holds the password
    let reports = sink.reported();
    assert_eq!(reports.len(), 1, "the call is reported");
    let shown = format!("{} | {}", reports[0].command, reports[0].stderr_tail);
    assert!(!shown.contains("s3cret-token"), "{shown}");
}
