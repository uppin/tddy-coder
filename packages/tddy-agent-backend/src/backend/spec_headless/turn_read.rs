//! Reading one spawned headless turn to its end: stdout through the adapter, stderr kept, the
//! conversation log appended.


use crate::adapters::StreamSinks;
use crate::backend::spec_headless::TurnRead;
use crate::backend::turn_process::STDERR_GRACE;
use crate::backend::{InvokeRequest, InvokeResponse};
use crate::error::BackendError;
use crate::stream::{self, ProgressEvent};
use std::io::{BufReader, Write as _};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Reads one spawned turn to its end on a blocking thread: stdout through the adapter, the exit
/// through the adapter's judgement, stderr kept, the conversation log appended.
pub(super) fn read_turn(
    read: TurnRead,
    mut child: std::process::Child,
    stdout: Option<std::process::ChildStdout>,
    stderr: Option<std::process::ChildStderr>,
    reaped: &AtomicBool,
) -> Result<InvokeResponse, BackendError> {
    let TurnRead {
        request,
        invocation,
        agent_id,
        system_text,
        progress_callback,
        mut conversation,
    } = read;
    let Some(stdout) = stdout else {
        // Never the case for a piped stdout; the child is still reaped, so none is left behind.
        let _ = child.kill();
        let _ = child.wait();
        reaped.store(true, Ordering::SeqCst);
        return Err(BackendError::InvocationFailed(
            "failed to capture stdout".to_string(),
        ));
    };
    let stderr_reader = stderr.map(read_stderr);

    let emit_progress = {
        let progress_sink = request.progress_sink.clone();
        move |event: &ProgressEvent| {
            if let Some(sink) = &progress_sink {
                sink.emit(event);
            } else if let Some(callback) = &progress_callback {
                if let Ok(mut callback) = callback.lock() {
                    callback(event);
                }
            }
        }
    };
    let mut on_progress = emit_progress.clone();
    let agent_output = request.agent_output;
    let agent_output_sink = request.agent_output_sink.clone();
    let mut on_raw_output = move |text: &str| {
        if agent_output {
            if let Some(sink) = &agent_output_sink {
                sink.emit(text);
            } else if std::env::var("TDDY_QUIET").is_err() {
                eprint!("{text}");
            }
        }
    };
    let debug = request.debug;
    let mut on_debug_line = |line: &str| {
        if debug {
            log::debug!("[tddy-coder debug] {line}");
        }
    };
    if let Some(file) = conversation.as_mut() {
        let (session_id, is_resume) = request
            .session
            .as_ref()
            .map(|s| (s.session_id().to_string(), s.is_resume()))
            .unwrap_or_default();
        let entry = serde_json::json!({
            "type": "tddy-request",
            "goal": request.hints.display_name,
            "prompt": request.prompt,
            "system_prompt": system_text,
            "model": request.model,
            "session_id": session_id,
            "is_resume": is_resume,
        });
        let _ = writeln!(file, "{entry}");
        let _ = file.flush();
    }
    let mut on_conversation_line = |line: &str| {
        if let Some(file) = conversation.as_mut() {
            let _ = writeln!(file, "{line}");
            let _ = file.flush();
        }
    };
    let has_conversation = request.conversation_output_path.is_some();

    let stream_result = invocation.adapter.read_stream(
        &mut BufReader::new(stdout),
        StreamSinks {
            on_progress: &mut on_progress,
            on_raw_output: &mut on_raw_output,
            on_debug_line: if debug {
                Some(&mut on_debug_line)
            } else {
                None
            },
            on_conversation_line: if has_conversation {
                Some(&mut on_conversation_line)
            } else {
                None
            },
        },
    );
    let status = child
        .wait()
        .map_err(|e| BackendError::InvocationFailed(e.to_string()));
    reaped.store(true, Ordering::SeqCst);
    let stream_result = stream_result?;
    let status = status?;
    let stderr = stderr_reader.map(StderrReader::finish).unwrap_or_default();
    let exit_code = status.code().unwrap_or(-1);
    log::debug!(
        "[headless] {agent_id} exited with code {exit_code} (goal: {:?})",
        request.goal_id
    );

    if !invocation
        .adapter
        .exit_ok(exit_code, &stream_result, invocation.exit_ok_if_structured)
    {
        let detail = invocation.adapter.failure_detail(&stream_result, &stderr);
        return Err(BackendError::InvocationFailed(if detail.is_empty() {
            format!("{agent_id} exited with code {exit_code}")
        } else {
            format!("{agent_id} exited with code {exit_code}: {detail}")
        }));
    }

    emit_progress(&stream::ProgressEvent::AgentExited {
        exit_code,
        goal: request.submit_key.to_string(),
    });
    Ok(InvokeResponse {
        output: stream_result.result_text,
        exit_code,
        session_id: Some(stream_result.session_id).filter(|id| !id.is_empty()),
        questions: stream_result.questions,
        raw_stream: if stream_result.raw_lines.is_empty() {
            None
        } else {
            Some(stream_result.raw_lines.join("\n"))
        },
        stderr: Some(stderr).filter(|stderr| !stderr.trim().is_empty()),
    })
}

/// The request's conversation log, opened for appending, when it names one.
pub(super) fn open_conversation_log(
    request: &InvokeRequest,
) -> Result<Option<std::fs::File>, BackendError> {
    request
        .conversation_output_path
        .as_ref()
        .map(|path| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| {
                    BackendError::InvocationFailed(format!(
                        "failed to open conversation output {}: {e}",
                        path.display()
                    ))
                })
        })
        .transpose()
}

/// A child's stderr, read to its end on a thread of its own.
struct StderrReader {
    collected: Arc<Mutex<Vec<u8>>>,
    done: std::sync::mpsc::Receiver<()>,
}

fn read_stderr(mut stderr: std::process::ChildStderr) -> StderrReader {
    let collected: Arc<Mutex<Vec<u8>>> = Arc::default();
    let (sender, done) = std::sync::mpsc::channel();
    let sink = Arc::clone(&collected);
    std::thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        loop {
            match std::io::Read::read(&mut stderr, &mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(read) => sink
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .extend_from_slice(&chunk[..read]),
            }
        }
        let _ = sender.send(());
    });
    StderrReader { collected, done }
}

impl StderrReader {
    /// What the child wrote to stderr, once it has closed it — or, after a bounded grace (a
    /// grandchild can hold the pipe open), what had been read by then.
    fn finish(self) -> String {
        let _ = self.done.recv_timeout(STDERR_GRACE);
        let bytes = self
            .collected
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        String::from_utf8_lossy(&bytes).into_owned()
    }
}
