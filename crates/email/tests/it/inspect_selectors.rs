//! v2-dkim-legacy-urls C14: `EmailInspect::inspect` passes the request's `dkim_selectors` to the
//! inspection, so a key published at a user selector reaches the `email.dkim` evidence block.
//! Needs `--features testing` (the stub inspector).

use netray_email::config::RateLimitConfig;
use netray_email::job::{EmailInspect, EmailInspectRequest, InspectSlots};
use netray_email::security::RateLimitState;
use netray_email::testing::{example_dns, stub_inspector};
use netray_model::evidence::{Shape, Value};

// Throwaway 2048-bit RSA SPKI, base64 (same key as the dkim check's unit tests).
const RSA_2048_P: &str = "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAv06e3j461rNqkC2J1zvYTmm2ttXIUL3plNVJDrUdqHnB14KnAbMOz2RCdBwdDEi1BZ6uESPzTkKSz/EdkDHRSaULx6XzgEGhAfXiXiVgWeh5e+fhNjVkFSquPx57PuYZAnaotyNi9ppQs/MyF+XTdHTl4GQfJG0y8Wyaj6gndzGubPaoZq3gI5j0hoqsVDpwzraw9BtWZVUMhhhIhDrITiJCY8PCmqFcFCOH22T+0zFFEor+mfkagBUOfF6cw/QVD/6MLxK/Ull2Tvi8xtW0crx5YHACMZDedLfVkCX6JokHCFHu2fSem/2T/HxQn1MD73uW2ZKTsgGJ3cWZsHYC/QIDAQAB";

#[tokio::test]
async fn request_dkim_selectors_reach_the_dkim_evidence_block() {
    let record = format!("v=DKIM1; k=rsa; p={RSA_2048_P}");
    let dns = example_dns().with_txt("custom1._domainkey.example.com", vec![record.as_str()]);
    let inspector = stub_inspector(dns);
    let limits = RateLimitState::new(&serde_json::from_str::<RateLimitConfig>("{}").unwrap())
        .expect("default rate limit");
    let slots = InspectSlots::new(4);

    let job = inspector
        .inspect(
            &EmailInspectRequest {
                domain: "example.com".into(),
                dkim_selectors: vec!["custom1".into()],
            },
            "198.51.100.1".parse().unwrap(),
            &limits,
            &slots,
        )
        .expect("inspection starts");
    let result = job.collect().await.expect("a summary");

    let block = result
        .evidence
        .iter()
        .find(|b| b.id.to_string() == "email.dkim")
        .expect("an email.dkim block");
    let Shape::Table { rows, .. } = &block.shape else {
        panic!("email.dkim is not a table: {block:?}");
    };
    let keys: Vec<String> = rows
        .iter()
        .filter_map(|r| match r.first() {
            Some(Value::Text(t)) => Some(t.clone()),
            _ => None,
        })
        .collect();
    assert!(
        keys.iter().any(|k| k.starts_with("custom1#")),
        "no custom1# row in {keys:?}"
    );
}
