//! The closes running on this daemon, one per session: closes of different sessions run at once,
//! and a second close of a session whose close is still running is refused, naming that close's
//! task (docs/ft/daemon/session-liveness.md § Closing a session).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use tddy_rpc::Status;

/// The running close of each session, by session id.
#[derive(Default)]
pub(crate) struct SessionCloses {
    running: Mutex<HashMap<String, RunningClose>>,
    next_reservation: AtomicU64,
}

/// One running close: which reservation holds the session, and its task once it is named.
struct RunningClose {
    reservation: u64,
    task_id: Option<String>,
}

impl SessionCloses {
    /// Hold `session_id` for a close until the returned reservation is dropped; refused
    /// `ALREADY_EXISTS`, naming the running close's task, while another close holds it.
    pub(crate) fn reserve(self: &Arc<Self>, session_id: &str) -> Result<CloseReservation, Status> {
        let mut running = self.lock();
        if let Some(close) = running.get(session_id) {
            return Err(Status::already_exists(match &close.task_id {
                Some(task_id) => {
                    format!("session {session_id} is already being closed by task {task_id}")
                }
                None => {
                    format!("session {session_id} is already being closed; its task is starting")
                }
            }));
        }
        let reservation = self.next_reservation.fetch_add(1, Ordering::Relaxed);
        running.insert(
            session_id.to_string(),
            RunningClose {
                reservation,
                task_id: None,
            },
        );
        Ok(CloseReservation {
            closes: Arc::clone(self),
            session_id: session_id.to_string(),
            reservation,
        })
    }

    /// Refused `ALREADY_EXISTS`, naming the close, while a close holds `session_id`.
    pub(crate) fn refuse_while_closing(&self, session_id: &str) -> Result<(), Status> {
        match self.lock().get(session_id) {
            None => Ok(()),
            Some(close) => Err(Status::already_exists(match &close.task_id {
                Some(task_id) => format!(
                    "session {session_id} is being closed by task {task_id}; wait for the close \
                     to end"
                ),
                None => format!("session {session_id} is being closed; wait for the close to end"),
            })),
        }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, RunningClose>> {
        // A map of plain records: a thread that panicked holding it left nothing half-written.
        self.running.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl tddy_agent_launch::launch_ports::SessionClosesInFlight for SessionCloses {
    fn refuse_while_closing(&self, session_id: &str) -> Result<(), Status> {
        SessionCloses::refuse_while_closing(self, session_id)
    }
}

/// A session held for one close; dropping it lets the next close of the session start.
pub(crate) struct CloseReservation {
    closes: Arc<SessionCloses>,
    session_id: String,
    reservation: u64,
}

impl CloseReservation {
    /// The close is running as task `task_id`: a refused second close names it.
    pub(crate) fn runs_as(&self, task_id: &str) {
        if let Some(close) = self.closes.lock().get_mut(&self.session_id) {
            if close.reservation == self.reservation {
                close.task_id = Some(task_id.to_string());
            }
        }
    }
}

impl Drop for CloseReservation {
    fn drop(&mut self) {
        let mut running = self.closes.lock();
        if running
            .get(&self.session_id)
            .is_some_and(|close| close.reservation == self.reservation)
        {
            running.remove(&self.session_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_close_of_a_session_is_refused_naming_the_running_task_until_it_ends() {
        // Given a close of one session running as a task
        let closes = Arc::new(SessionCloses::default());
        let first = closes.reserve("s-1").expect("the first close is held");
        first.runs_as("task-7");

        // When a second close of it, and one of another session, are asked for
        let second = closes
            .reserve("s-1")
            .err()
            .map(|status| status.message().to_string());
        // Dropped at once: only whether the other session could be held matters here.
        let other = closes.reserve("s-2").is_ok();
        drop(first);
        let after = closes.reserve("s-1").is_ok();

        // Then
        assert_eq!(
            (
                second.as_deref().map(|message| message.contains("task-7")),
                other,
                after
            ),
            (Some(true), true, true)
        );
    }

    #[test]
    fn a_session_held_by_a_close_refuses_other_work_until_the_close_ends() {
        // Given a close of one session running as a task
        let closes = Arc::new(SessionCloses::default());
        let close = closes.reserve("s-1").expect("the close is held");
        close.runs_as("task-7");

        // When work on it, and on another session, is asked for while it runs, and after
        let while_closing = closes
            .refuse_while_closing("s-1")
            .err()
            .map(|status| (status.code(), status.message().contains("task-7")));
        let another = closes.refuse_while_closing("s-2").is_ok();
        drop(close);
        let after = closes.refuse_while_closing("s-1").is_ok();

        // Then
        assert_eq!(
            (while_closing, another, after),
            (Some((tddy_rpc::Code::AlreadyExists, true)), true, true)
        );
    }
}
