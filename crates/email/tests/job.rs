// V2 Phase 6, requirements 29 and 31 (crate half): the email job called directly, with no axum
// type, over the offline stub inspector. Needs `[[test]] name = "job"` with
// `required-features = ["testing"]` on this test target.

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr};

use futures::StreamExt;
use netray_email::quality::types::{Category, SseEvent};
use netray_email::security::RateLimitState;
use netray_email::security::rate_limit::{Quotas, Scopes};
use netray_email::testing::{example_dns, stub_inspector};
use netray_email::{
    EmailInspect, EmailInspectRequest, EmailJob, EmailRefusal, InspectSlots, translate,
};
use netray_engine::SectionOutcome;

const CALLER: IpAddr = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 7));

fn limiter(per_minute: u32, burst: u32) -> RateLimitState {
    RateLimitState::scoped(
        &Quotas {
            per_ip_per_minute: per_minute,
            per_ip_burst: burst,
        },
        Scopes {
            metrics_prefix: "job_test",
            per_ip: "per_ip",
        },
    )
}

fn start(
    domain: &str,
    limits: &RateLimitState,
    slots: &InspectSlots,
) -> Result<EmailJob, EmailRefusal> {
    let request = EmailInspectRequest {
        domain: domain.to_string(),
        dkim_selectors: Vec::new(),
    };
    stub_inspector(example_dns()).inspect(&request, CALLER, limits, slots)
}

async fn events_of(domain: &str) -> Vec<SseEvent> {
    let job = start(domain, &limiter(10, 10), &InspectSlots::new(4))
        .ok()
        .expect("job starts");
    StreamExt::collect::<Vec<_>>(job).await
}

fn category_names(events: &[SseEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            SseEvent::Category(c) => Some(serde_json::to_string(&c.category).unwrap()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn job_streams_every_category_then_evidence_then_summary() {
    let events = events_of("example.com").await;

    assert_eq!(events.len(), 12 + 2, "12 categories, evidence, summary");
    assert!(matches!(events[12], SseEvent::Evidence { .. }));
    assert!(matches!(events[13], SseEvent::Summary { .. }));
    match &events[12] {
        SseEvent::Evidence { blocks } => assert!(!blocks.is_empty(), "evidence is non-empty"),
        _ => unreachable!(),
    }

    let names = category_names(&events[..12]);
    assert_eq!(names.len(), 12);
    let got: HashSet<String> = names.into_iter().collect();
    let want: HashSet<String> = Category::ALL
        .iter()
        .map(|c| serde_json::to_string(c).unwrap())
        .collect();
    assert_eq!(got.len(), 12, "each category once");
    assert_eq!(got, want);
}

#[tokio::test]
async fn collect_equals_the_streamed_events() {
    let events = events_of("example.com").await;

    let job = start("example.com", &limiter(10, 10), &InspectSlots::new(4))
        .ok()
        .expect("job starts");
    let result = job.collect().await.expect("a summary arrives");

    let mut streamed: Vec<serde_json::Value> = events
        .iter()
        .filter_map(|e| match e {
            SseEvent::Category(c) => Some(serde_json::to_value(c).unwrap()),
            _ => None,
        })
        .collect();
    let mut collected: Vec<serde_json::Value> = result
        .categories
        .iter()
        .map(|c| serde_json::to_value(c).unwrap())
        .collect();
    let key = |v: &serde_json::Value| v["category"].as_str().unwrap().to_string();
    streamed.sort_by_key(key);
    collected.sort_by_key(key);
    assert_eq!(collected, streamed);

    let (blocks, grade, verdicts) = (
        events
            .iter()
            .find_map(|e| match e {
                SseEvent::Evidence { blocks } => Some(blocks),
                _ => None,
            })
            .unwrap(),
        events.iter().find_map(|e| match e {
            SseEvent::Summary { grade, .. } => Some(*grade),
            _ => None,
        }),
        events.iter().find_map(|e| match e {
            SseEvent::Summary { verdicts, .. } => Some(verdicts),
            _ => None,
        }),
    );
    assert_eq!(
        serde_json::to_value(&result.evidence).unwrap(),
        serde_json::to_value(blocks).unwrap()
    );
    assert_eq!(Some(result.summary.grade), grade);
    assert_eq!(Some(&result.summary.verdicts), verdicts);
}

#[tokio::test]
async fn translate_of_the_job_events_is_measured() {
    let events = events_of("example.com").await;
    assert!(matches!(
        translate(&events),
        SectionOutcome::Measured { .. }
    ));
}

#[tokio::test]
async fn an_ip_literal_starts_and_ends_with_a_summary() {
    let events = events_of("192.0.2.1").await;
    assert!(matches!(events.last(), Some(SseEvent::Summary { .. })));
}

#[tokio::test]
async fn a_malformed_domain_is_invalid_input() {
    let refusal = start("not a domain", &limiter(10, 10), &InspectSlots::new(4))
        .err()
        .expect("refused");
    assert_eq!(
        refusal,
        EmailRefusal::InvalidInput(
            "invalid domain: domain must have at least two labels".to_string()
        )
    );
}

#[tokio::test]
async fn the_second_job_over_a_burst_of_one_is_rate_limited() {
    let limits = limiter(1, 1);
    let slots = InspectSlots::new(4);
    let first = start("example.com", &limits, &slots);
    assert!(first.is_ok(), "the first job starts");
    let second = start("example.com", &limits, &slots)
        .err()
        .expect("refused");
    assert!(
        matches!(second, EmailRefusal::RateLimited { retry_after_secs } if retry_after_secs >= 1),
        "got {second:?}"
    );
}

#[tokio::test]
async fn a_held_job_takes_the_only_slot_until_it_is_dropped() {
    let limits = limiter(10, 10);
    let slots = InspectSlots::new(1);
    let first = start("example.com", &limits, &slots).ok().expect("first");
    assert_eq!(slots.available(), 0);
    let second = start("example.com", &limits, &slots).err().expect("busy");
    assert_eq!(second, EmailRefusal::Busy);
    assert_eq!(slots.available(), 0);

    drop(first);
    assert!(start("example.com", &limits, &slots).is_ok());
}
