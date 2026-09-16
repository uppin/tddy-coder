//! One operation, run against the service **in process**, and the status the process exits with.
//!
//! This is the half of the binary that justifies the shape of the whole crate. The generated trait
//! takes and returns prost structs inside [`tddy_rpc::Request`]/[`tddy_rpc::Response`], so calling
//! it from here passes Rust values: nothing is encoded, nothing is decoded, no socket is dialled,
//! and — the point — none of the operations' logic is duplicated for the command line. A
//! single-shot run and a served request reach the same `CodeIndexServiceImpl` method.
//!
//! What is *not* shared is what a result means, because that is the caller's to decide. A check
//! with findings and a comparison that does not hold are both answered calls whose answer is a
//! failed run; the service returns them as values and this is the caller that turns them into a
//! non-zero exit — the same judgement `restructure_cli::report` makes for `tddy-tools`.

use futures_util::StreamExt;
use tddy_index_daemon::proto::code_index::{restructure_event, CodeIndexService, RestructureEvent};
use tddy_index_daemon::{CodeIndexServiceImpl, EventStream};

use crate::cli::Requested;
use crate::render;

/// What a single-shot run amounted to, as the status the process exits with.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// The operation was answered and the answer is one to carry on from.
    Held,
    /// The operation was refused, or answered with something a caller must not ignore.
    Refused,
}

impl Verdict {
    pub(crate) fn exit_code(&self) -> std::process::ExitCode {
        match self {
            Verdict::Held => std::process::ExitCode::SUCCESS,
            Verdict::Refused => std::process::ExitCode::FAILURE,
        }
    }
}

/// Run `requested` against `service` and render what it answered.
pub(crate) async fn run_once(service: &CodeIndexServiceImpl, requested: Requested) -> Verdict {
    match requested {
        Requested::Check(request) => {
            let streamed = service.check(tddy_rpc::Request::new(request)).await;
            checked(streamed).await
        }
        Requested::Apply(request) => {
            let streamed = service.apply(tddy_rpc::Request::new(request)).await;
            applied(streamed).await
        }
        Requested::Anchors(request) => match service.anchors(tddy_rpc::Request::new(request)).await
        {
            Ok(response) => {
                render::anchors(&response.into_inner());
                Verdict::Held
            }
            Err(refusal) => refused(&refusal),
        },
        Requested::PlanStatus(request) => {
            match service.plan_status(tddy_rpc::Request::new(request)).await {
                Ok(response) => {
                    render::plan_status(&response.into_inner());
                    Verdict::Held
                }
                Err(refusal) => refused(&refusal),
            }
        }
        Requested::Verify(request) => match service.verify(tddy_rpc::Request::new(request)).await {
            Ok(response) => {
                let comparison = response.into_inner();
                render::verify(&comparison);
                // A comparison that does not hold is the answer, not an error — and what this
                // caller means by it is a failed run.
                if comparison.holds {
                    Verdict::Held
                } else {
                    Verdict::Refused
                }
            }
            Err(refusal) => refused(&refusal),
        },
    }
}

/// A check's stream, rendered, with its findings counted into the verdict.
async fn checked(
    streamed: Result<tddy_rpc::Response<EventStream<RestructureEvent>>, tddy_rpc::Status>,
) -> Verdict {
    let mut found = 0usize;
    let drained = drain(streamed, |event| {
        if matches!(event.event, Some(restructure_event::Event::Finding(_))) {
            found += 1;
        }
    })
    .await;

    if drained == Verdict::Refused {
        return drained;
    }
    // Only a check that was *answered* reports its findings: a refused one never got far enough to
    // know whether the plan has any.
    render::findings(found);
    if found == 0 {
        Verdict::Held
    } else {
        Verdict::Refused
    }
}

/// An apply's stream, rendered. Everything it has to say about how far it got is in the events.
async fn applied(
    streamed: Result<tddy_rpc::Response<EventStream<RestructureEvent>>, tddy_rpc::Status>,
) -> Verdict {
    drain(streamed, |_| {}).await
}

/// Render every event a stream carried, letting `watch` see each one, and report whether the run
/// was refused.
///
/// A refusal is the last item in the stream rather than the call's return value whenever the work
/// had already started, which is why both are folded into one verdict here.
async fn drain(
    streamed: Result<tddy_rpc::Response<EventStream<RestructureEvent>>, tddy_rpc::Status>,
    mut watch: impl FnMut(&RestructureEvent),
) -> Verdict {
    let mut stream = match streamed {
        Ok(response) => response.into_inner(),
        Err(refusal) => return refused(&refusal),
    };

    let mut verdict = Verdict::Held;
    while let Some(next) = stream.next().await {
        match next {
            Ok(event) => {
                watch(&event);
                render::restructure(&event);
            }
            Err(refusal) => verdict = refused(&refusal),
        }
    }
    verdict
}

/// Report a refusal, and say that it failed the run.
fn refused(status: &tddy_rpc::Status) -> Verdict {
    render::refusal(status);
    Verdict::Refused
}
