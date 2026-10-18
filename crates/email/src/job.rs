use std::collections::HashMap;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::Stream;
use netray_common::enrichment::EnrichmentClient;
use serde::{Deserialize, Serialize};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tracing::Instrument;
use utoipa::ToSchema;

use crate::checks::run_all_checks;
use crate::config::Config;
use crate::dns::{DnsLookup, DnsResolver};
use crate::error::MailError;
use crate::input;
use crate::quality::types::{CheckResult, Grade, SseEvent, Verdict};
use crate::security::RateLimitState;
use crate::state::{AppState, OutboundFetch};

/// Everything `run_all_checks` needs except the domain, the selectors and the channel.
pub struct EmailInspector<R: DnsLookup + 'static = DnsResolver> {
    config: Arc<Config>,
    dns: Arc<R>,
    dnsbl: Arc<R>,
    fetch: OutboundFetch,
    enrichment: Option<Arc<EnrichmentClient>>,
}

impl<R: DnsLookup + 'static> Clone for EmailInspector<R> {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            dns: self.dns.clone(),
            dnsbl: self.dnsbl.clone(),
            fetch: self.fetch.clone(),
            enrichment: self.enrichment.clone(),
        }
    }
}

impl<R: DnsLookup + 'static> EmailInspector<R> {
    pub fn new(
        config: Arc<Config>,
        dns: Arc<R>,
        dnsbl: Arc<R>,
        fetch: OutboundFetch,
        enrichment: Option<Arc<EnrichmentClient>>,
    ) -> Self {
        Self {
            config,
            dns,
            dnsbl,
            fetch,
            enrichment,
        }
    }

    /// Start an inspection in beacon's order: domain, selectors, per-IP limit, slot. Synchronous;
    /// must run inside a tokio runtime.
    pub fn start(
        &self,
        raw_domain: &str,
        selectors: Vec<String>,
        client: IpAddr,
        limits: &RateLimitState,
        slots: &InspectSlots,
    ) -> Result<EmailJob, MailError> {
        let domain = input::parse_domain(raw_domain)?;
        input::validate_selectors(&selectors, self.config.dkim.max_user_selectors)?;
        limits.check(client)?;
        let permit = slots
            .0
            .clone()
            .try_acquire_owned()
            .map_err(|_| MailError::TooManyConcurrent)?;

        let (tx, rx) = mpsc::channel::<SseEvent>(32);
        let handle = tokio::spawn(
            run_all_checks(
                domain.clone(),
                selectors,
                self.config.clone(),
                self.dns.clone(),
                self.dnsbl.clone(),
                self.fetch.clone(),
                self.enrichment.clone(),
                tx,
            )
            .instrument(tracing::Span::current()),
        );

        Ok(EmailJob {
            domain,
            events: ReceiverStream::new(rx),
            handle,
            _permit: permit,
        })
    }
}

impl AppState {
    /// The inspector over beacon's own config, resolvers, fetch and enrichment.
    pub fn inspector(&self) -> EmailInspector {
        EmailInspector::new(
            self.config.clone(),
            self.dns_resolver.clone(),
            self.dnsbl_resolver.clone(),
            self.fetch.clone(),
            self.enrichment_client.clone(),
        )
    }
}

/// An email inspection a service can start without knowing the resolver type.
pub trait EmailInspect: Send + Sync + 'static {
    fn inspect(
        &self,
        req: &EmailInspectRequest,
        client: IpAddr,
        limits: &RateLimitState,
        slots: &InspectSlots,
    ) -> Result<EmailJob, EmailRefusal>;
}

impl<R: DnsLookup + 'static> EmailInspect for EmailInspector<R> {
    fn inspect(
        &self,
        req: &EmailInspectRequest,
        client: IpAddr,
        limits: &RateLimitState,
        slots: &InspectSlots,
    ) -> Result<EmailJob, EmailRefusal> {
        self.start(&req.domain, Vec::new(), client, limits, slots)
            .map_err(Into::into)
    }
}

/// A running inspection: beacon's events in emission order. Dropping it aborts the task and
/// frees the slot.
pub struct EmailJob {
    domain: String,
    events: ReceiverStream<SseEvent>,
    handle: JoinHandle<()>,
    _permit: OwnedSemaphorePermit,
}

impl Stream for EmailJob {
    type Item = SseEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.events).poll_next(cx)
    }
}

impl Drop for EmailJob {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

impl EmailJob {
    /// The domain `parse_domain` made of the request.
    pub fn domain(&self) -> &str {
        &self.domain
    }

    /// Read to the first `Summary`: every category in arrival order, the evidence blocks (empty
    /// when none came), the summary. `None` when the stream ends without a summary.
    pub async fn collect(mut self) -> Option<EmailInspectResult> {
        let mut categories = Vec::new();
        let mut evidence = Vec::new();
        while let Some(event) = self.next().await {
            match event {
                SseEvent::Category(c) => categories.push(c),
                SseEvent::Evidence { blocks } => evidence = blocks,
                SseEvent::Summary {
                    grade,
                    verdicts,
                    duration_ms,
                } => {
                    return Some(EmailInspectResult {
                        categories,
                        evidence,
                        summary: EmailSummary {
                            grade,
                            verdicts,
                            duration_ms,
                        },
                    });
                }
            }
        }
        None
    }
}

/// The concurrency bulkhead: refuses when every slot is taken.
#[derive(Clone)]
pub struct InspectSlots(Arc<Semaphore>);

impl InspectSlots {
    pub fn new(permits: usize) -> Self {
        Self(Arc::new(Semaphore::new(permits)))
    }

    pub fn shared(semaphore: Arc<Semaphore>) -> Self {
        Self(semaphore)
    }

    pub fn available(&self) -> usize {
        self.0.available_permits()
    }
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EmailInspectRequest {
    #[schema(example = "example.com")]
    pub domain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EmailInspectResult {
    pub categories: Vec<CheckResult>,
    pub evidence: Vec<netray_model::evidence::Block>,
    pub summary: EmailSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EmailSummary {
    pub grade: Grade,
    pub verdicts: HashMap<String, Verdict>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EmailRefusal {
    #[error("{0}")]
    InvalidInput(String),
    #[error("rate limit exceeded")]
    RateLimited { retry_after_secs: u64 },
    #[error("every email inspection slot is taken")]
    Busy,
    #[error("{0}")]
    Unavailable(String),
}

impl From<MailError> for EmailRefusal {
    fn from(e: MailError) -> Self {
        match e {
            MailError::InvalidDomain(_)
            | MailError::InvalidSelector { .. }
            | MailError::TooManySelectors { .. } => Self::InvalidInput(e.to_string()),
            MailError::RateLimited {
                retry_after_secs, ..
            } => Self::RateLimited { retry_after_secs },
            MailError::TooManyConcurrent => Self::Busy,
            MailError::DnsError(_) | MailError::Config(_) => Self::Unavailable(e.to_string()),
        }
    }
}
