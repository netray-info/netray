#[cfg(test)]
mod tests {
    //! Golden for the body of `GET /json?ip=<addr>`, consumed by lens's contract test
    //! (`crates/lens/tests/contract_ip.rs`). UPDATE_GOLDEN=1 rewrites it.

    use crate::backend::{CloudInfo, Ifconfig, Ip, Location, Network};
    use crate::format::OutputFormat;
    use crate::handlers;

    const GOLDEN: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/contracts/ifconfig-json.json"
    );

    fn sample() -> Ifconfig {
        let mut location = Location::unknown();
        location.city = Some("Berlin".into());
        location.region = Some("Berlin".into());
        location.region_code = Some("BE".into());
        location.country = Some("Germany".into());
        location.country_iso = Some("DE".into());
        location.postal_code = Some("10115".into());
        location.is_eu = Some(true);
        location.latitude = Some(52.52);
        location.longitude = Some(13.405);
        location.timezone = Some("Europe/Berlin".into());
        location.continent = Some("Europe".into());
        location.continent_code = Some("EU".into());
        Ifconfig {
            ip: Ip {
                addr: "203.0.113.42".into(),
                version: "4".into(),
                hostname: None,
            },
            // An explicit ?ip= query has no TCP peer and no User-Agent of the target.
            tcp: None,
            location,
            network: Network {
                asn: Some(64496),
                org: Some("Example Cloud GmbH".into()),
                prefix: Some("203.0.113.0/24".into()),
                asn_category: Some("hosting".into()),
                network_role: Some("stub".into()),
                asn_registered: Some("2010-01-01".into()),
                network_type: "cloud".into(),
                infra_type: "cloud".into(),
                is_internal: false,
                is_datacenter: true,
                is_vpn: false,
                is_tor: false,
                is_bot: false,
                is_c2: false,
                is_spamhaus: false,
                cloud: Some(CloudInfo {
                    provider: "aws".into(),
                    service: Some("EC2".into()),
                    region: Some("eu-central-1".into()),
                }),
                vpn: None,
                bot: None,
                is_anycast: false,
                is_cins: false,
                iana_label: None,
            },
            user_agent: None,
        }
    }

    #[test]
    fn json_ip_query_matches_golden() {
        // Same path as GET /json?ip=: handlers::root::to_json, then the JSON output format.
        let value = handlers::root::to_json(&sample()).expect("ifconfig serializes");
        let mut body = OutputFormat::Json.serialize_body(&value).expect("json body");
        body.push('\n');

        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            std::fs::write(GOLDEN, &body).expect("write golden");
            return;
        }
        let want = std::fs::read_to_string(GOLDEN).unwrap_or_else(|_| {
            panic!("golden {GOLDEN} missing; run UPDATE_GOLDEN=1 cargo test -p ifconfig-rs --lib contract_golden")
        });
        assert_eq!(
            body, want,
            "/json?ip= shape changed; if intended, run UPDATE_GOLDEN=1 cargo test -p ifconfig-rs --lib contract_golden"
        );
    }
}
