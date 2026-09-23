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

use std::future::Future;

use futures_util::StreamExt;
use tddy_index_daemon::proto::code_index::{
    restructure_event, AnalyzeEvent, CodeIndexService, RestructureEvent,
};
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
            let streamed = service.check(tddy_rpc::Request::direct(request)).await;
            checked(streamed).await
        }
        Requested::Apply(request) => {
            // Read before the request is handed over, because the summary at the end of the run
            // has to say whether the tree was written to and the events do not carry that.
            let rehearsal = request.dry_run;
            let streamed = service.apply(tddy_rpc::Request::direct(request)).await;
            applied(streamed, rehearsal).await
        }
        Requested::Anchors(request) => {
            // Likewise: the anchor document names the file the range is in, and the answer carries
            // the range alone.
            let file = request.file.clone();
            match service.anchors(tddy_rpc::Request::direct(request)).await {
                Ok(response) => {
                    render::anchors(&file, &response.into_inner());
                    Verdict::Held
                }
                Err(refusal) => refused(&refusal),
            }
        }
        Requested::PlanStatus(request) => {
            match service
                .plan_status(tddy_rpc::Request::direct(request))
                .await
            {
                Ok(response) => {
                    render::plan_status(&response.into_inner());
                    Verdict::Held
                }
                Err(refusal) => refused(&refusal),
            }
        }
        Requested::Coverage(request) => {
            let streamed = service.coverage(tddy_rpc::Request::direct(request)).await;
            analysed(streamed, interrupted()).await
        }
        Requested::DuplicateTests(request) => {
            let streamed = service
                .duplicate_tests(tddy_rpc::Request::direct(request))
                .await;
            analysed(streamed, interrupted()).await
        }
        Requested::Report(request) => {
            match service.report(tddy_rpc::Request::direct(request)).await {
                Ok(response) => {
                    render::report(&response.into_inner());
                    Verdict::Held
                }
                Err(refusal) => refused(&refusal),
            }
        }
        Requested::Complexity(request) => {
            match service.complexity(tddy_rpc::Request::direct(request)).await {
                Ok(response) => {
                    render::complexity(&response.into_inner());
                    Verdict::Held
                }
                Err(refusal) => refused(&refusal),
            }
        }
        Requested::Verify(request) => {
            match service.verify(tddy_rpc::Request::direct(request)).await {
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
            }
        }
    }
}

/// A check's stream, rendered, with its findings counted into the verdict.
async fn checked(
    streamed: Result<tddy_rpc::Response<EventStream<RestructureEvent>>, tddy_rpc::Status>,
) -> Verdict {
    let mut found = 0usize;
    // A check writes nothing whatever it finds, so nothing it reports is an apply's summary and
    // the rehearsal flag has nothing to change.
    let drained = drain(streamed, false, |event| {
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
    rehearsal: bool,
) -> Verdict {
    drain(streamed, rehearsal, |_| {}).await
}

/// This process's own interrupt, as the future an analysis is raced against.
///
/// Only the analyses take one: `^C` on a 55-minute capture or a ~22-minute detection is the one
/// place a single-shot run has something to stop that outlives the keystroke, and the operations
/// that finish in seconds are better served by the default disposition.
async fn interrupted() {
    // `let _ =` as `serve` does it: a listener this process could not register would leave `^C`
    // with its default disposition, which is the behaviour a run had before this existed.
    let _ = tokio::signal::ctrl_c().await;
}

/// Render every event an analysis reported, until it ends or `interrupt` resolves.
///
/// An interrupt's answer is to **drop the stream**, which is the whole point: a send into a
/// dropped receiver is the only signal the service gets that its caller has gone, and it turns
/// that into the cancellation predicate the capture checks between tests. So `^C` here stops the
/// work rather than only this loop, and the run exits non-zero because a capture that stopped part
/// way is not one to carry on from.
async fn analysed(
    streamed: Result<tddy_rpc::Response<EventStream<AnalyzeEvent>>, tddy_rpc::Status>,
    interrupt: impl Future<Output = ()>,
) -> Verdict {
    let mut stream = match streamed {
        Ok(response) => response.into_inner(),
        Err(refusal) => return refused(&refusal),
    };
    let mut interrupt = std::pin::pin!(interrupt);

    let mut verdict = Verdict::Held;
    loop {
        tokio::select! {
            next = stream.next() => match next {
                Some(Ok(event)) => render::analyze(&event),
                Some(Err(refusal)) => verdict = refused(&refusal),
                None => return verdict,
            },
            () = &mut interrupt => {
                render::interrupted();
                return Verdict::Refused;
            }
        }
    }
}

/// Render every event a stream carried, letting `watch` see each one, and report whether the run
/// was refused.
///
/// A refusal is the last item in the stream rather than the call's return value whenever the work
/// had already started, which is why both are folded into one verdict here.
async fn drain(
    streamed: Result<tddy_rpc::Response<EventStream<RestructureEvent>>, tddy_rpc::Status>,
    rehearsal: bool,
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
                render::restructure(&event, rehearsal);
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

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_index_daemon::proto::code_index::{analyze_event, AnalyzeEvent, CaptureFinished};
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;

    /// What the service hands a single-shot run, as the call it came from returned it.
    type Streamed = Result<tddy_rpc::Response<EventStream<AnalyzeEvent>>, tddy_rpc::Status>;

    /// One analysis stream, with its sending half kept so a test can ask what became of it.
    fn an_analysis_in_flight() -> (
        mpsc::Sender<Result<AnalyzeEvent, tddy_rpc::Status>>,
        Streamed,
    ) {
        let (events, receiver) = mpsc::channel(4);
        (
            events,
            Ok(tddy_rpc::Response::new(ReceiverStream::new(receiver))),
        )
    }

    /// Nothing will interrupt this run.
    async fn never_interrupted() {
        std::future::pending().await
    }

    /// The link that makes `^C` stop a 55-minute capture rather than only this loop: the drain's
    /// answer to an interrupt is to **drop the stream**, and a send into a dropped receiver is the
    /// only signal the service gets that its caller has gone.
    #[tokio::test]
    async fn an_interrupted_analysis_drops_the_stream_it_was_reading() {
        // Given a capture that has reported nothing yet — its build phase — and an operator who
        // has just pressed ^C
        let (events, streamed) = an_analysis_in_flight();

        // When the run is drained against that interrupt
        let verdict = analysed(streamed, std::future::ready(())).await;

        // Then the run failed and the stream is gone, so the capture's own cancellation predicate
        // turns true at the next test it would have run
        assert_eq!(verdict, Verdict::Refused);
        assert!(
            events.is_closed(),
            "an interrupted run left its stream open, so the work would carry on for nobody"
        );
    }

    #[tokio::test]
    async fn a_capture_that_ran_to_its_last_event_is_a_run_to_carry_on_from() {
        // Given a capture that finished and wrote its denominator
        let (events, streamed) = an_analysis_in_flight();
        events
            .send(Ok(AnalyzeEvent {
                event: Some(analyze_event::Event::CaptureFinished(CaptureFinished {
                    tests: 2014,
                    files: 109,
                })),
            }))
            .await
            .expect("the stream takes its terminal event");
        drop(events);

        // When the run is drained
        let verdict = analysed(streamed, never_interrupted()).await;

        // Then it succeeded
        assert_eq!(verdict, Verdict::Held);
    }

    /// The other end of the same cancellation: a capture the service stopped reports it as a
    /// refusal on the stream, and a command line must exit non-zero rather than report a partial
    /// capture as a success.
    #[tokio::test]
    async fn an_analysis_the_service_cancelled_fails_the_run() {
        // Given a capture the service stopped because nobody was listening any more
        let (events, streamed) = an_analysis_in_flight();
        events
            .send(Err(tddy_rpc::Status::deadline_exceeded(
                "coverage capture was cancelled before it finished: 312 test(s) captured, and no \
                 denominator was written",
            )))
            .await
            .expect("the stream takes its refusal");
        drop(events);

        // When the run is drained
        let verdict = analysed(streamed, never_interrupted()).await;

        // Then the run failed
        assert_eq!(verdict, Verdict::Refused);
    }
}
