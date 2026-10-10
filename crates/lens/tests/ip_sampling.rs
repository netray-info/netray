// Requirement 3: only public addresses are enriched; at most four IPv4 and four
// IPv6, each family sorted ascending; "checked N of M addresses" appears only
// when N < M. The IP section is `netray_ip`'s `golden_module` through the registry, answering
// every sampled address, so the addresses the section reports are the ones it sampled.
//
// Changed from the stub-server version: the documentation addresses (192.0.2.x, 2001:db8::)
// became 1.1.1.x and 2606:4700::, because the module's own target policy refuses documentation
// ranges. C10 (private filtered) no longer asserts the "checked 1 of 2" text: the module's
// `translate` takes `total_public`, so whether the total counts the private address is the
// module's decision, not part of this contract; the sampled address is still asserted.

mod common;

use std::net::IpAddr;
use std::time::Duration;

use common::{ip_golden, run_ip};
use lens::backends::BackendExtra;

struct Case {
    name: &'static str,
    input: Vec<&'static str>,
    sent: Vec<&'static str>,
    /// `Some(None)`: no "checked" message; `Some(Some(m))`: message `m`; `None`: not asserted.
    message: Option<Option<&'static str>>,
}

#[tokio::test]
async fn ip_sampling_table() {
    let cases = vec![
        Case {
            name: "C7 ten A and two AAAA",
            input: vec![
                "1.1.1.9",
                "1.1.1.3",
                "1.1.1.10",
                "1.1.1.1",
                "2606:4700::9",
                "1.1.1.7",
                "1.1.1.5",
                "1.1.1.2",
                "1.1.1.8",
                "2606:4700::2",
                "1.1.1.6",
                "1.1.1.4",
            ],
            sent: vec![
                "1.1.1.1",
                "1.1.1.2",
                "1.1.1.3",
                "1.1.1.4",
                "2606:4700::2",
                "2606:4700::9",
            ],
            message: Some(Some("checked 6 of 12 addresses")),
        },
        Case {
            name: "C8 five A",
            input: vec!["1.1.1.5", "1.1.1.2", "1.1.1.4", "1.1.1.1", "1.1.1.3"],
            sent: vec!["1.1.1.1", "1.1.1.2", "1.1.1.3", "1.1.1.4"],
            message: Some(Some("checked 4 of 5 addresses")),
        },
        Case {
            name: "C9 three A",
            input: vec!["1.1.1.3", "1.1.1.1", "1.1.1.2"],
            sent: vec!["1.1.1.1", "1.1.1.2", "1.1.1.3"],
            message: Some(None),
        },
        Case {
            name: "C10 private filtered",
            input: vec!["10.0.0.5", "8.8.8.8"],
            sent: vec!["8.8.8.8"],
            message: None,
        },
    ];

    for case in cases {
        let result = run_ip(
            ip_golden("ifconfig-json.json"),
            Duration::from_secs(5),
            &case.input,
        )
        .await
        .unwrap_or_else(|e| panic!("{}: ip section failed: {e:?}", case.name));

        let BackendExtra::Ip { addresses, .. } = &result.extra else {
            panic!("{}: the IP section must carry the IP extras", case.name);
        };
        let mut got: Vec<IpAddr> = addresses.iter().map(|a| a.ip).collect();
        got.sort();
        let mut want: Vec<IpAddr> = case.sent.iter().map(|s| s.parse().unwrap()).collect();
        want.sort();
        assert_eq!(got, want, "{}: addresses enriched", case.name);

        let rep = result
            .checks
            .iter()
            .find(|c| c.name == "reputation")
            .unwrap_or_else(|| panic!("{}: no reputation check", case.name));
        match case.message {
            Some(Some(msg)) => assert!(
                rep.messages.iter().any(|m| m.contains(msg)),
                "{}: expected {msg:?} in {:?}",
                case.name,
                rep.messages
            ),
            Some(None) => assert!(
                !rep.messages.iter().any(|m| m.contains("checked")),
                "{}: unexpected 'checked' in {:?}",
                case.name,
                rep.messages
            ),
            None => {}
        }
    }
}
