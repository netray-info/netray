// V2 unmeasured, requirement 6 second bullet, scenarios 7 and 8: a nameserver that does not
// answer is not a verdict on the zone. `dns.ns_lame` and `dns.ns_delegation` stay measured while
// at least one nameserver answers (the silent one is named in a finding), and are `Unmeasured`
// when none answers. An NS host name that does not exist (NXDOMAIN) is a defect, not a gap.
//
// Assumed interface (the one item the tests need, in `netray_dns::testing`, feature `testing`):
//
//     pub enum NsStub { Authoritative, Timeout, NxDomain }
//     pub async fn ns_check_outcomes(domain: &str, ns: &[(&str, NsStub)])
//         -> Vec<netray_model::CheckResult>
//
// It runs the production NS lame-delegation and delegation-consistency checks against stubbed
// raw queries (one stub per NS host name; `Authoritative` answers SOA with AA=1 and the full NS
// set, `Timeout` never answers, `NxDomain` makes the host name unresolvable) and returns the
// translated `dns.ns_lame` and `dns.ns_delegation` results. No network.
//
// Cargo needs on netray-dns: `[[test]] name = "ns_timeouts"`, `path = "tests/ns_timeouts.rs"`,
// `required-features = ["testing"]`.

use netray_dns::testing::{NsStub, ns_check_outcomes};
use netray_model::{CheckResult, Status};

const DOMAIN: &str = "example.com";
const CHECKS: [&str; 2] = ["dns.ns_lame", "dns.ns_delegation"];

fn outcome<'a>(results: &'a [CheckResult], id: &str) -> &'a CheckResult {
    results
        .iter()
        .find(|r| r.id.to_string() == id)
        .unwrap_or_else(|| panic!("{id} missing from {results:?}"))
}

#[tokio::test]
async fn silent_nameservers_decide_measured_or_unmeasured() {
    // (case, stubs, expected status, a finding must name this host)
    let table: [(&str, [(&str, NsStub); 2], Status, Option<&str>); 2] = [
        (
            "one answers, one times out",
            [
                ("ns1.example.com", NsStub::Authoritative),
                ("ns2.example.com", NsStub::Timeout),
            ],
            Status::Pass,
            Some("ns2.example.com"),
        ),
        (
            "both time out",
            [
                ("ns1.example.com", NsStub::Timeout),
                ("ns2.example.com", NsStub::Timeout),
            ],
            Status::Unmeasured,
            None,
        ),
    ];

    for (case, stubs, want, silent) in table {
        let results = ns_check_outcomes(DOMAIN, &stubs).await;
        for id in CHECKS {
            let r = outcome(&results, id);
            assert_eq!(r.status, want, "{case}: {id}: {r:?}");
            assert!(
                !r.findings.is_empty(),
                "{case}: {id} needs a finding: {r:?}"
            );
            if let Some(host) = silent {
                assert!(
                    r.findings.iter().any(|f| f.contains(host)),
                    "{case}: {id} finding must name {host}: {:?}",
                    r.findings
                );
            }
        }
    }
}

#[tokio::test]
async fn nxdomain_nameserver_stays_a_measured_defect() {
    let results = ns_check_outcomes(DOMAIN, &[("gone.example.com", NsStub::NxDomain)]).await;
    for id in CHECKS {
        let r = outcome(&results, id);
        assert!(
            matches!(r.status, Status::Warn | Status::Fail),
            "{id}: NXDOMAIN is a defect, not unmeasured: {r:?}"
        );
    }
}

#[test]
fn lame_catalogue_finding_no_longer_claims_reached() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../engine/catalogue/checks.toml"
    );
    let toml: toml::Value =
        toml::from_str(&std::fs::read_to_string(path).expect("read checks.toml"))
            .expect("parse checks.toml");
    let entry = toml["check"]
        .as_array()
        .expect("[[check]] array")
        .iter()
        .find(|c| c["id"].as_str() == Some("dns.ns_lame"))
        .expect("dns.ns_lame entry");
    let finding = entry["finding"].as_str().expect("finding string");
    assert!(!finding.contains("or reached"), "finding: {finding}");
}
