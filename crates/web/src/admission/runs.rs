//! The [`DomainRuns`] the result page and API v1 run domains through, over admission. Copied
//! from lens's `api_v1.rs`.

use std::net::IpAddr;

use futures::future::BoxFuture;

use super::{Admit, ApexState, Reason, Refusal, RunStream, Trigger};
use crate::api::{
    DomainRefusal, DomainRuns, RefusalReason, RunEvent, RunRefusal, RunStart, RunTrigger,
};
use crate::error::validate_domain;

pub struct AdmittedRuns {
    pub state: ApexState,
}

impl AdmittedRuns {
    pub fn new(state: ApexState) -> Self {
        Self { state }
    }
}

impl DomainRuns for AdmittedRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        validate_domain(domain).map_err(|e| DomainRefusal {
            code: "DOMAIN_INVALID",
            message: e.to_string(),
        })
    }

    fn start<'a>(
        &'a self,
        domain: &'a str,
        client: IpAddr,
        trigger: RunTrigger,
        prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        Box::pin(async move {
            if let Err(r) = super::check_limits(&self.state, client) {
                super::count_rejection(&r);
                return RunStart::Refused(refusal(r));
            }
            let trigger = match trigger {
                RunTrigger::Visit => Trigger::Visit,
                RunTrigger::Recheck => Trigger::Recheck,
                RunTrigger::Api => Trigger::Api,
            };
            match super::admit(&self.state, domain, client, trigger, prefetch) {
                Admit::Admitted(stream) => {
                    if !stream.joined() {
                        self.state.client_runs.record(client);
                    }
                    RunStart::Admitted(Box::pin(futures::stream::unfold(stream, events)))
                }
                Admit::Refused(r) => RunStart::Refused(refusal(r)),
                Admit::NotStarted => RunStart::NotStarted,
            }
        })
    }
}

async fn events(mut stream: RunStream) -> Option<(RunEvent, RunStream)> {
    let event = match stream.next().await? {
        super::RunEvent::Section(ev) => RunEvent::Section(ev),
        super::RunEvent::Done(published) => RunEvent::Done(published.run.clone()),
        super::RunEvent::Recorded(r) => RunEvent::Recorded(r),
    };
    Some((event, stream))
}

fn refusal(r: Refusal) -> RunRefusal {
    RunRefusal {
        reason: match r.reason {
            Reason::Throttle => RefusalReason::Throttle,
            Reason::Budget => RefusalReason::Budget,
            Reason::IpLimit => RefusalReason::IpLimit,
            Reason::GlobalLimit => RefusalReason::GlobalLimit,
            Reason::Slots => RefusalReason::Slots,
        },
        retry_after_s: r.retry_after_s,
        last_run_ago_s: r.last_run_ago_s,
        last_incomplete: r.last_incomplete,
    }
}
