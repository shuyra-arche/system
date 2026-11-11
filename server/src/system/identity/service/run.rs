use kernel_api::{abi::call::sign::SignReply, model::wait::Wait};
use kernel_client::call::{room, sign};
use system_protocol::{
    identity::operations::{DecodedRequest, Response},
    value::Outcome,
};
use weave::{server::ServeError, transport::ReceiveError};

use super::{Config, Error, Failure, Phase, Service, answer, inspect};

#[derive(Debug, Default)]
pub struct Report {
    pub handled: usize,
    pub rejected: usize,
    pub reply_failures: usize,
    pub first_reply_failure: Option<crate::protocol::ServeFailure>,
    pub stopped: bool,
}

/// Own one complete service lifecycle. Failures return the live resource owner
/// so its caller can finish and retry cleanup rather than dropping obligations.
#[allow(
    clippy::result_large_err,
    reason = "retain service resources without allocating on failure"
)]
pub fn serve(config: Config) -> Result<(), Failure> {
    let mut service = Service::create(config)?;
    loop {
        match service.step(Wait::Infinite) {
            Ok(report) if report.stopped => {
                return match service.finish() {
                    Ok(()) => Ok(()),
                    Err(error) => Err(Failure {
                        error: Error::Cleanup(error),
                        service,
                    }),
                };
            }
            Ok(report) => {
                if report.reply_failures != 0 {
                    let text = b"identity: reply delivery failed\n";
                    let _ = kernel_client::call::debug::put(
                        kernel_api::address::VirtAddr::new(text.as_ptr().addr()),
                        text.len(),
                    );
                }
            }
            Err(error) => return Err(Failure { error, service }),
        }
    }
}

pub(super) fn step(service: &mut Service, wait: Wait) -> Result<Report, Error> {
    if !matches!(service.phase, Phase::Running) {
        return Err(Error::Inactive);
    }
    // Liveness comes from actual owned grants. No ambient Join permission or
    // untrusted numeric TaskId is treated as an execution observation handle.
    inspect(service.config.shutdown)?;
    inspect(service.config.dependency)?;
    match sign::listen(service.config.shutdown, Wait::Poll).map_err(Error::Sign)? {
        SignReply::Triggered { sequence } => {
            sign::consume(service.config.shutdown, sequence).map_err(Error::Sign)?;
            service.phase = Phase::Closing;
            return Ok(Report {
                stopped: true,
                ..Report::default()
            });
        }
        SignReply::Pending => (),
        _ => return Err(Error::InvalidConfig),
    }
    service
        .revision
        .flush(service.config.notify)
        .map_err(Error::Revision)?;
    let mut report = Report::default();
    let mut retained = false;
    let count = service.faces.len();
    for offset in 0..count {
        let index = (service.cursor + offset) % count;
        let face = &mut service.faces[index];
        let root = face.root.ok_or(Error::Inactive)?;
        let grant = face.grant;
        let endpoint = face.endpoint.as_mut().ok_or(Error::Inactive)?;
        let mut changed_error = None;
        let book = service.book.as_mut().ok_or(Error::Inactive)?;
        let result = endpoint.serve_once(Wait::Poll, |caller, request| {
            let mutation = mutating(&request);
            let response = answer::answer(
                book,
                answer::Context {
                    from: caller.into(),
                    endpoint: root.into(),
                    grant,
                },
                request,
            );
            if mutation
                && successful(&response)
                && let Err(error) = service
                    .revision
                    .record()
                    .and_then(|()| service.revision.flush(service.config.notify))
            {
                changed_error = Some(error);
            }
            response
        });
        if let Some(error) = changed_error {
            if matches!(error, super::revision::Error::Exhausted) {
                service.phase = Phase::Closing;
            }
            return Err(Error::Revision(error));
        }
        match result {
            Ok(()) => report.handled += 1,
            Err(crate::protocol::Error::NotReady) => {
                retained |= endpoint.needs_retry();
            }
            Err(crate::protocol::Error::Serve(ServeError::Receive(ReceiveError::Decode)))
            | Err(crate::protocol::Error::Serve(ServeError::Route(_))) => report.rejected += 1,
            Err(crate::protocol::Error::Serve(error @ ServeError::Send(_))) => {
                // The request has already executed. Do not redispatch it or let
                // a caller closing/filling its reply queue kill the service.
                report.handled += 1;
                report.reply_failures += 1;
                if report.first_reply_failure.is_none() {
                    report.first_reply_failure = Some(error);
                }
            }
            Err(error) => return Err(Error::Ingress(error)),
        }
    }
    service.cursor = (service.cursor + 1) % count;
    if report.handled + report.rejected != 0 {
        return Ok(report);
    }
    if retained {
        room::yield_();
    } else if wait != Wait::Poll {
        // Routed readiness remains level-triggered. A source event only wakes
        // the loop; all authority and actual readiness are checked next step.
        service
            .waiting
            .as_ref()
            .ok_or(Error::Inactive)?
            .await_(wait)
            .map_err(Error::Waiting)?;
    }
    Ok(report)
}

fn mutating(request: &DecodedRequest) -> bool {
    matches!(
        request,
        DecodedRequest::Adopt(_)
            | DecodedRequest::Waive(_)
            | DecodedRequest::Restrict(_)
            | DecodedRequest::Derive(_)
            | DecodedRequest::Found(_)
            | DecodedRequest::Admit(_)
            | DecodedRequest::Expel(_)
            | DecodedRequest::Bind(_)
            | DecodedRequest::Unbind(_)
            | DecodedRequest::Activate(_)
            | DecodedRequest::Install(_)
            | DecodedRequest::Cancel(_)
            | DecodedRequest::Retire(_)
            | DecodedRequest::Associate(_)
    )
}

fn successful(response: &Response) -> bool {
    matches!(
        response,
        Response::Adopt(Outcome::Success(_))
            | Response::Waive(Outcome::Success(_))
            | Response::Restrict(Outcome::Success(_))
            | Response::Derive(Outcome::Success(_))
            | Response::Found(Outcome::Success(_))
            | Response::Admit(Outcome::Success(_))
            | Response::Expel(Outcome::Success(_))
            | Response::Bind(Outcome::Success(_))
            | Response::Unbind(Outcome::Success(_))
            | Response::Activate(Outcome::Success(_))
            | Response::Install(Outcome::Success(_))
            | Response::Cancel(Outcome::Success(_))
            | Response::Retire(Outcome::Success(_))
            | Response::Associate(Outcome::Success(_))
    )
}
