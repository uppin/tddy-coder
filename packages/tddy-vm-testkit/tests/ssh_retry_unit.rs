//! Unit tests for the retry policy behind the testkit's SSH channel.
//!
//! The distinction here is the one that mattered in practice. QEMU's slirp networking
//! accepts a forwarded connection whether or not anything in the guest is listening, so
//! *reaching* sshd genuinely needs retrying — but `TestHostVm::deploy` used the same
//! retrying call to run `sudo ./install --systemd` with a 600 s budget, so a genuinely
//! failing, non-idempotent install was re-executed for ten minutes and only the last
//! attempt's output was ever reported.
//!
//! Time is paused, so these assert on attempt *counts* rather than on wall-clock behaviour:
//! the cadence is virtual and each test finishes in microseconds.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::time::Duration;

use anyhow::Result;
use pretty_assertions::assert_eq;
use tddy_vm::vm::VerifyResult;
use tddy_vm_testkit::guest::{retry_until_successful, GuestCommandOutput};

/// The budget a probe is given, and the gap between its attempts. Virtual under a paused
/// clock: five gaps fit inside this budget.
const BUDGET: Duration = Duration::from_secs(10);
const GAP: Duration = Duration::from_secs(2);

/// An SSH channel that answers a scripted sequence of exit codes and then repeats the last
/// one for as long as it is asked, counting every attempt made against it.
///
/// Repeating the tail is what lets a test script "fails twice, then succeeds" without also
/// having to predict how many attempts the retry loop will make.
struct ScriptedSsh {
    remaining: RefCell<VecDeque<i32>>,
    last: Cell<i32>,
    attempts: Cell<u32>,
}

/// An SSH channel answering `exit_codes` in order, then repeating the last of them.
fn an_ssh_answering(exit_codes: &[i32]) -> ScriptedSsh {
    let last = *exit_codes
        .last()
        .expect("a scripted SSH channel must be given at least one exit code to answer");
    ScriptedSsh {
        remaining: RefCell::new(exit_codes.iter().copied().collect()),
        last: Cell::new(last),
        attempts: Cell::new(0),
    }
}

impl ScriptedSsh {
    async fn attempt(&self) -> Result<GuestCommandOutput> {
        self.attempts.set(self.attempts.get() + 1);
        let exit_code = match self.remaining.borrow_mut().pop_front() {
            Some(exit_code) => exit_code,
            None => self.last.get(),
        };
        self.last.set(exit_code);
        Ok(GuestCommandOutput::from_ssh(
            "true",
            VerifyResult {
                success: exit_code == 0,
                stdout: format!("attempt {} exited {exit_code}", self.attempts.get()),
                stderr: String::new(),
                exit_code,
            },
        ))
    }

    fn attempts(&self) -> u32 {
        self.attempts.get()
    }
}

#[tokio::test(start_paused = true)]
async fn runs_a_command_once_when_it_succeeds_on_the_first_attempt() {
    // Given an SSH channel that answers successfully straight away
    let ssh = an_ssh_answering(&[0]);

    // When it is polled until success
    let result = retry_until_successful(BUDGET, GAP, || ssh.attempt())
        .await
        .expect("the probe must run");

    // Then nothing was retried: a command that worked is not run again
    assert_eq!((ssh.attempts(), result.exit_code()), (1, 0));
}

#[tokio::test(start_paused = true)]
async fn stops_retrying_as_soon_as_an_attempt_succeeds() {
    // Given an SSH channel that refuses twice — sshd not listening yet — and then answers
    let ssh = an_ssh_answering(&[255, 255, 0]);

    // When it is polled until success
    let result = retry_until_successful(BUDGET, GAP, || ssh.attempt())
        .await
        .expect("the probe must run");

    // Then it stopped at the first success rather than spending the rest of the budget
    assert_eq!((ssh.attempts(), result.exit_code()), (3, 0));
}

#[tokio::test(start_paused = true)]
async fn keeps_retrying_a_failing_command_until_the_budget_is_spent() {
    // Given an SSH channel that never succeeds
    let ssh = an_ssh_answering(&[1]);

    // When it is polled until success
    let result = retry_until_successful(BUDGET, GAP, || ssh.attempt())
        .await
        .expect("the probe must run");

    // Then the whole budget was spent on it — one attempt, then one per gap until the
    // deadline — and the last outcome is what comes back. This is the behaviour a readiness
    // probe wants and the reason a non-idempotent command must never be run through it
    assert_eq!((ssh.attempts(), result.exit_code()), (6, 1));
}
