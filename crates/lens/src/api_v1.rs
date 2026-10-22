//! lens's side of the API v1 seam: the `DomainRuns` the apex API runs domains through.
use std::net::IpAddr;

use futures::future::BoxFuture;
use netray_web::api::{
    DomainRefusal, DomainRuns, RefusalReason, RunEvent, RunRefusal, RunStart, RunTrigger,
};

use crate::admission::{self, Admit, Reason, Refusal, RunStream, Trigger};
use crate::check::CheckInput;
use crate::input;
use crate::state::AppState;

pub struct LensRuns {
    pub state: AppState,
}

impl LensRuns {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }
}

impl DomainRuns for LensRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        input::validate_domain(domain).map_err(|e| DomainRefusal {
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
            if let Err(r) = admission::check_limits(&self.state, client) {
                admission::count_rejection(&r);
                return RunStart::Refused(refusal(r));
            }
            let trigger = match trigger {
                RunTrigger::Visit => Trigger::Visit,
                RunTrigger::Recheck => Trigger::Recheck,
                RunTrigger::Api => Trigger::Api,
            };
            match admission::admit(&self.state, domain, client, trigger, prefetch) {
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

    /// A run with DKIM selectors: never joins and is never joined, not recorded (V1's
    /// `admit_dkim`); a prefetch starts none.
    fn start_with<'a>(
        &'a self,
        domain: &'a str,
        client: IpAddr,
        trigger: RunTrigger,
        prefetch: bool,
        dkim_selectors: &'a [String],
    ) -> BoxFuture<'a, RunStart> {
        if dkim_selectors.is_empty() {
            return self.start(domain, client, trigger, prefetch);
        }
        Box::pin(async move {
            if let Err(r) = admission::check_limits(&self.state, client) {
                return RunStart::Refused(refusal(r));
            }
            if prefetch {
                return RunStart::NotStarted;
            }
            let input = CheckInput {
                domain: domain.to_string(),
                dkim_selectors: Some(dkim_selectors.to_vec()),
                client_ip: Some(client),
                request_id: None,
            };
            match admission::admit_dkim(&self.state, input) {
                Ok(stream) => {
                    self.state.client_runs.record(client);
                    RunStart::Admitted(Box::pin(futures::stream::unfold(stream, events)))
                }
                Err(r) => RunStart::Refused(refusal(r)),
            }
        })
    }
}

async fn events(mut stream: RunStream) -> Option<(RunEvent, RunStream)> {
    let event = match stream.next().await? {
        admission::RunEvent::Section(ev) => RunEvent::Section(ev),
        admission::RunEvent::Done(published) => RunEvent::Done(published.run.clone()),
        admission::RunEvent::Recorded(r) => RunEvent::Recorded(r),
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
