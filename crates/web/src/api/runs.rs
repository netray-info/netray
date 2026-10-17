//! The seam between the API and the run machinery (spec v2-api requirement 1).
use std::net::IpAddr;
use std::pin::Pin;

use futures::Stream;
use futures::future::BoxFuture;

pub trait DomainRuns: Send + Sync + 'static {
    /// The normalised domain, or the refusal.
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal>;

    fn start<'a>(
        &'a self,
        domain: &'a str,
        client: IpAddr,
        trigger: RunTrigger,
        prefetch: bool,
    ) -> BoxFuture<'a, RunStart>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainRefusal {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunTrigger {
    Visit,
    Recheck,
    Api,
}

pub enum RunStart {
    Admitted(RunEvents),
    Refused(RunRefusal),
    NotStarted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRefusal {
    pub reason: RefusalReason,
    pub retry_after_s: u64,
    pub last_run_ago_s: Option<u64>,
    pub last_incomplete: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalReason {
    Throttle,
    Budget,
    IpLimit,
    GlobalLimit,
    Slots,
}

pub type RunEvents = Pin<Box<dyn Stream<Item = RunEvent> + Send>>;

pub enum RunEvent {
    Section(netray_engine::SectionEvent),
    Done(netray_model::run::Run),
    Recorded(Option<netray_store::Recorded>),
}
