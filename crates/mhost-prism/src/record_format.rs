//! Record human-readable formatting.
//!
//! Provides human-readable formatting for DNS record types (TXT, CAA, MX, SOA)
//! and [`enrich_lookups_json`] for injecting `*_human` fields into serialized
//! `BatchEvent` JSON values.

use mhost::resources::rdata::TXT;
use mhost::resources::rdata::parsed_txt::{Mechanism, Modifier, ParsedTxt, Qualifier, Word};

/// Decode a TXT record to a human-readable string.
///
/// Attempts to parse the TXT content as a known type (SPF, DMARC, BIMI,
/// MTA-STS, TLS-RPT, domain verification). Falls back to the plain UTF-8
/// string from `txt.as_string()` if parsing fails.
pub fn format_txt(txt: &TXT) -> String {
    let text = txt.as_string();
    match ParsedTxt::from_str(&text) {
        Ok(ParsedTxt::Spf(spf)) => {
            let mut lines = vec![format!("SPF: v=spf{}", spf.version())];
            for word in spf.words() {
                match word {
                    Word::Word(q, mechanism) => {
                        let q_sym = match q {
                            Qualifier::Pass => "+",
                            Qualifier::Neutral => "?",
                            Qualifier::Softfail => "~",
                            Qualifier::Fail => "-",
                        };
                        let mech_str = match mechanism {
                            Mechanism::All => "all".to_string(),
                            Mechanism::A { domain_spec, cidr_len } => {
                                let mut s = "a".to_string();
                                if let Some(d) = domain_spec {
                                    s = format!("a:{d}");
                                }
                                if let Some(c) = cidr_len {
                                    s = format!("{s}/{c}");
                                }
                                s
                            }
                            Mechanism::IPv4(ip) => format!("ip4:{ip}"),
                            Mechanism::IPv6(ip) => format!("ip6:{ip}"),
                            Mechanism::MX { domain_spec, cidr_len } => {
                                let mut s = "mx".to_string();
                                if let Some(d) = domain_spec {
                                    s = format!("mx:{d}");
                                }
                                if let Some(c) = cidr_len {
                                    s = format!("{s}/{c}");
                                }
                                s
                            }
                            Mechanism::PTR(d) => match d {
                                Some(d) => format!("ptr:{d}"),
                                None => "ptr".to_string(),
                            },
                            Mechanism::Exists(d) => format!("exists:{d}"),
                            Mechanism::Include(d) => format!("include:{d}"),
                        };
                        lines.push(format!("  {q_sym} {mech_str}"));
                    }
                    Word::Modifier(modifier) => match modifier {
                        Modifier::Redirect(d) => lines.push(format!("  redirect={d}")),
                        Modifier::Exp(d) => lines.push(format!("  exp={d}")),
                    },
                }
            }
            lines.join("\n")
        }
        Ok(ParsedTxt::Dmarc(dmarc)) => {
            let mut lines = vec![
                format!("DMARC: v={}", dmarc.version()),
                format!("  Policy: {}", dmarc.policy()),
            ];
            if let Some(sp) = dmarc.subdomain_policy() {
                lines.push(format!("  Subdomain Policy: {sp}"));
            }
            if let Some(rua) = dmarc.rua() {
                lines.push(format!("  Aggregate Reports: {}", strip_uri_scheme(rua)));
            }
            if let Some(ruf) = dmarc.ruf() {
                lines.push(format!("  Forensic Reports: {}", strip_uri_scheme(ruf)));
            }
            if let Some(adkim) = dmarc.adkim() {
                lines.push(format!("  DKIM Alignment: {adkim}"));
            }
            if let Some(aspf) = dmarc.aspf() {
                lines.push(format!("  SPF Alignment: {aspf}"));
            }
            if let Some(pct) = dmarc.pct() {
                lines.push(format!("  Percentage: {pct}%"));
            }
            if let Some(fo) = dmarc.fo() {
                lines.push(format!("  Failure Options: {fo}"));
            }
            if let Some(ri) = dmarc.ri() {
                lines.push(format!("  Report Interval: {ri}s"));
            }
            lines.join("\n")
        }
        Ok(ParsedTxt::MtaSts(mta_sts)) => {
            format!("MTA-STS: v={}\n  id: {}", mta_sts.version(), mta_sts.id())
        }
        Ok(ParsedTxt::TlsRpt(tls_rpt)) => {
            format!("TLS-RPT: v={}\n  rua: {}", tls_rpt.version(), tls_rpt.rua())
        }
        Ok(ParsedTxt::Bimi(bimi)) => {
            let mut lines = vec![format!("BIMI: v={}", bimi.version())];
            if let Some(logo) = bimi.logo() {
                lines.push(format!("  logo: {logo}"));
            }
            if let Some(authority) = bimi.authority() {
                lines.push(format!("  authority: {authority}"));
            }
            lines.join("\n")
        }
        Ok(ParsedTxt::DomainVerification(dv)) => {
            format!(
                "Verification: {}\n  scope: {}\n  id: {}",
                dv.verifier(),
                dv.scope(),
                dv.id()
            )
        }
        Err(_) => text,
    }
}

/// Strip common URI schemes (`mailto:`, `https://`) for cleaner display.
fn strip_uri_scheme(s: &str) -> &str {
    s.strip_prefix("mailto:")
        .or_else(|| s.strip_prefix("https://"))
        .unwrap_or(s)
}

/// Format a CAA record as a human-readable policy string.
pub fn format_caa_human(obj: &serde_json::Value) -> Option<String> {
    let tag = obj.get("tag")?.as_str()?;
    let value = obj.get("value")?.as_str().unwrap_or("");
    let critical = obj
        .get("issuer_critical")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let policy = match (tag, value) {
        ("issue", "") => "No CA is allowed to issue certificates".to_string(),
        ("issue", v) => format!("Allow {v} to issue certificates"),
        ("issuewild", "") => {
            "No CA is allowed to issue wildcard certificates".to_string()
        }
        ("issuewild", v) => format!("Allow {v} to issue wildcard certificates"),
        ("iodef", v) => format!("Report policy violations to {}", strip_uri_scheme(v)),
        (t, v) => format!("{t} {v}"),
    };

    if critical {
        Some(format!("{policy} (critical)"))
    } else {
        Some(policy)
    }
}

/// Format an MX record as a human-readable string.
pub fn format_mx_human(obj: &serde_json::Value) -> Option<String> {
    let preference = obj.get("preference")?;
    let exchange = obj.get("exchange")?.as_str()?;
    Some(format!("{exchange} with priority {preference}"))
}

/// Format a SOA record as a human-readable string.
pub fn format_soa_human(obj: &serde_json::Value) -> Option<String> {
    let mname = obj.get("mname")?.as_str()?;
    let rname = obj.get("rname")?.as_str()?;
    let contact = rname_to_email(rname);
    let serial = obj.get("serial")?;
    let refresh = obj.get("refresh")?;
    let retry = obj.get("retry")?;
    let expire = obj.get("expire")?;
    let minimum = obj.get("minimum")?;
    Some(format!(
        "Primary NS: {mname}\nContact: {contact}\nSerial: {serial}\nRefresh: {refresh}\nRetry: {retry}\nExpire: {expire}\nMinimum TTL: {minimum}"
    ))
}

/// Convert SOA `rname` DNS encoding to email format.
///
/// In SOA records the first `.` in `rname` represents `@`.
/// E.g. `awsdns-hostmaster.amazon.com.` → `awsdns-hostmaster@amazon.com.`
fn rname_to_email(rname: &str) -> String {
    let rname = rname.strip_suffix('.').unwrap_or(rname);
    match rname.find('.') {
        Some(pos) => {
            let (local, rest) = rname.split_at(pos);
            format!("{local}@{}", &rest[1..])
        }
        None => rname.to_string(),
    }
}

/// Walk a serialized `BatchEvent` JSON value and inject human-readable fields
/// into record data objects within
/// `lookups.lookups[*].result.Response.records[*].data.*`.
///
/// Enrichments by record type:
/// - `"TXT"` / `"_dmarc"` → `txt_string` and `txt_human` into `data.TXT`
/// - `"CAA"` → `caa_human` into `data.CAA`
/// - `"MX"` → `mx_human` into `data.MX`
/// - `"SOA"` → `soa_human` into `data.SOA`
pub fn enrich_lookups_json(value: &mut serde_json::Value, record_type: &str) {
    match record_type {
        "TXT" | "_dmarc" => enrich_txt(value),
        "CAA" => enrich_simple(value, "CAA", format_caa_human, "caa_human"),
        "MX" => enrich_simple(value, "MX", format_mx_human, "mx_human"),
        "SOA" => enrich_simple(value, "SOA", format_soa_human, "soa_human"),
        _ => {}
    }
}

/// Enrich TXT records by decoding `txt_data` bytes and injecting `txt_string`
/// and `txt_human`.
fn enrich_txt(value: &mut serde_json::Value) {
    let lookup_count = value
        .get("lookups")
        .and_then(|l| l.get("lookups"))
        .and_then(|l| l.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    for li in 0..lookup_count {
        let record_count = value["lookups"]["lookups"][li]["result"]["Response"]["records"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);

        for ri in 0..record_count {
            let txt_string = {
                let txt_data = &value["lookups"]["lookups"][li]["result"]["Response"]["records"][ri]
                    ["data"]["TXT"]["txt_data"];
                let chunks = match txt_data.as_array() {
                    Some(arr) => arr,
                    None => continue,
                };
                chunks
                    .iter()
                    .map(|chunk| {
                        chunk
                            .as_array()
                            .map(|bytes| {
                                let raw: Vec<u8> = bytes
                                    .iter()
                                    .filter_map(|b| b.as_u64().map(|n| n as u8))
                                    .collect();
                                String::from_utf8_lossy(&raw).into_owned()
                            })
                            .unwrap_or_default()
                    })
                    .collect::<String>()
            };

            let txt = TXT::new(vec![txt_string.clone()]);
            let txt_human = format_txt(&txt);

            if let Some(obj) = value["lookups"]["lookups"][li]["result"]["Response"]["records"][ri]
                ["data"]["TXT"]
                .as_object_mut()
            {
                obj.insert("txt_string".to_string(), serde_json::Value::String(txt_string));
                obj.insert("txt_human".to_string(), serde_json::Value::String(txt_human));
            }
        }
    }
}

/// Enrich records of a simple structured type (CAA, MX, SOA) by reading the
/// data object, formatting it, and injecting the result.
fn enrich_simple(
    value: &mut serde_json::Value,
    data_key: &str,
    formatter: fn(&serde_json::Value) -> Option<String>,
    field_name: &str,
) {
    let lookup_count = value
        .get("lookups")
        .and_then(|l| l.get("lookups"))
        .and_then(|l| l.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    for li in 0..lookup_count {
        let record_count = value["lookups"]["lookups"][li]["result"]["Response"]["records"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);

        for ri in 0..record_count {
            let human = {
                let data = &value["lookups"]["lookups"][li]["result"]["Response"]["records"][ri]
                    ["data"][data_key];
                match formatter(data) {
                    Some(s) => s,
                    None => continue,
                }
            };

            if let Some(obj) = value["lookups"]["lookups"][li]["result"]["Response"]["records"][ri]
                ["data"][data_key]
                .as_object_mut()
            {
                obj.insert(field_name.to_string(), serde_json::Value::String(human));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_txt(s: &str) -> TXT {
        TXT::new(vec![s.to_string()])
    }

    #[test]
    fn format_spf_basic() {
        let txt = make_txt("v=spf1 ip4:192.0.2.0/24 -all");
        let out = format_txt(&txt);
        assert!(out.starts_with("SPF: v=spf1"), "got: {out}");
        assert!(out.contains("ip4:192.0.2.0/24"), "got: {out}");
        assert!(out.contains("- all"), "got: {out}");
    }

    #[test]
    fn format_dmarc_basic() {
        let txt = make_txt("v=DMARC1; p=reject; rua=mailto:dmarc@example.com");
        let out = format_txt(&txt);
        assert!(out.starts_with("DMARC: v=DMARC1"), "got: {out}");
        assert!(out.contains("Policy: reject"), "got: {out}");
        assert!(
            out.contains("Aggregate Reports: dmarc@example.com"),
            "got: {out}"
        );
    }

    #[test]
    fn format_mta_sts() {
        let txt = make_txt("v=STSv1; id=20190429T010101");
        let out = format_txt(&txt);
        assert!(out.starts_with("MTA-STS:"), "got: {out}");
        assert!(out.contains("id: 20190429T010101"), "got: {out}");
    }

    #[test]
    fn format_tls_rpt() {
        let txt = make_txt("v=TLSRPTv1; rua=mailto:tlsrpt@example.com");
        let out = format_txt(&txt);
        assert!(out.starts_with("TLS-RPT:"), "got: {out}");
        assert!(out.contains("rua: mailto:tlsrpt@example.com"), "got: {out}");
    }

    #[test]
    fn format_bimi() {
        let txt = make_txt("v=BIMI1; l=https://example.com/logo.svg");
        let out = format_txt(&txt);
        assert!(out.starts_with("BIMI:"), "got: {out}");
        assert!(out.contains("logo: https://example.com/logo.svg"), "got: {out}");
    }

    #[test]
    fn format_domain_verification() {
        let txt = make_txt("google-site-verification=abc123");
        let out = format_txt(&txt);
        assert!(out.starts_with("Verification:"), "got: {out}");
        assert!(out.contains("google"), "got: {out}");
        assert!(out.contains("abc123"), "got: {out}");
    }

    #[test]
    fn format_plain_fallback() {
        let txt = make_txt("some random text that is not parseable");
        let out = format_txt(&txt);
        assert_eq!(out, "some random text that is not parseable");
    }

    #[test]
    fn enrich_noop_for_unhandled_type() {
        let mut value = json!({
            "lookups": {
                "lookups": [{
                    "result": {
                        "Response": {
                            "records": [{
                                "data": { "A": "1.2.3.4" }
                            }]
                        }
                    }
                }]
            }
        });
        let before = value.clone();
        enrich_lookups_json(&mut value, "A");
        assert_eq!(value, before);
    }

    #[test]
    fn enrich_injects_txt_string_and_txt_human() {
        // "hello" = [104, 101, 108, 108, 111]
        let mut value = json!({
            "lookups": {
                "lookups": [{
                    "result": {
                        "Response": {
                            "records": [{
                                "data": {
                                    "TXT": {
                                        "txt_data": [[104, 101, 108, 108, 111]]
                                    }
                                }
                            }]
                        }
                    }
                }]
            }
        });
        enrich_lookups_json(&mut value, "TXT");
        let txt_obj = &value["lookups"]["lookups"][0]["result"]["Response"]["records"][0]["data"]["TXT"];
        assert_eq!(txt_obj["txt_string"], "hello");
        assert_eq!(txt_obj["txt_human"], "hello");
    }

    #[test]
    fn enrich_spf_record() {
        // "v=spf1 -all" encoded as bytes
        let bytes: Vec<serde_json::Value> = "v=spf1 -all"
            .bytes()
            .map(|b| serde_json::Value::Number(b.into()))
            .collect();
        let mut value = json!({
            "lookups": {
                "lookups": [{
                    "result": {
                        "Response": {
                            "records": [{
                                "data": {
                                    "TXT": {
                                        "txt_data": [bytes]
                                    }
                                }
                            }]
                        }
                    }
                }]
            }
        });
        enrich_lookups_json(&mut value, "TXT");
        let txt_obj = &value["lookups"]["lookups"][0]["result"]["Response"]["records"][0]["data"]["TXT"];
        assert_eq!(txt_obj["txt_string"], "v=spf1 -all");
        let human = txt_obj["txt_human"].as_str().unwrap();
        assert!(human.starts_with("SPF:"), "got: {human}");
    }

    #[test]
    fn enrich_dmarc_label() {
        let bytes: Vec<serde_json::Value> = "v=DMARC1; p=none"
            .bytes()
            .map(|b| serde_json::Value::Number(b.into()))
            .collect();
        let mut value = json!({
            "lookups": {
                "lookups": [{
                    "result": {
                        "Response": {
                            "records": [{
                                "data": {
                                    "TXT": {
                                        "txt_data": [bytes]
                                    }
                                }
                            }]
                        }
                    }
                }]
            }
        });
        enrich_lookups_json(&mut value, "_dmarc");
        let txt_obj = &value["lookups"]["lookups"][0]["result"]["Response"]["records"][0]["data"]["TXT"];
        let human = txt_obj["txt_human"].as_str().unwrap();
        assert!(human.starts_with("DMARC:"), "got: {human}");
    }

    #[test]
    fn enrich_multi_chunk_txt() {
        // Two chunks: "hello" + " world"
        let chunk1: Vec<serde_json::Value> = "hello".bytes().map(|b| serde_json::Value::Number(b.into())).collect();
        let chunk2: Vec<serde_json::Value> = " world".bytes().map(|b| serde_json::Value::Number(b.into())).collect();
        let mut value = json!({
            "lookups": {
                "lookups": [{
                    "result": {
                        "Response": {
                            "records": [{
                                "data": {
                                    "TXT": {
                                        "txt_data": [chunk1, chunk2]
                                    }
                                }
                            }]
                        }
                    }
                }]
            }
        });
        enrich_lookups_json(&mut value, "TXT");
        let txt_obj = &value["lookups"]["lookups"][0]["result"]["Response"]["records"][0]["data"]["TXT"];
        assert_eq!(txt_obj["txt_string"], "hello world");
    }

    // --- CAA formatting tests ---

    #[test]
    fn format_caa_issue_empty() {
        let obj = json!({"issuer_critical": false, "tag": "issue", "value": ""});
        let out = format_caa_human(&obj).unwrap();
        assert_eq!(out, "No CA is allowed to issue certificates");
    }

    #[test]
    fn format_caa_issue_ca() {
        let obj = json!({"issuer_critical": false, "tag": "issue", "value": "letsencrypt.org"});
        let out = format_caa_human(&obj).unwrap();
        assert_eq!(out, "Allow letsencrypt.org to issue certificates");
    }

    #[test]
    fn format_caa_issuewild_empty() {
        let obj = json!({"issuer_critical": false, "tag": "issuewild", "value": ""});
        let out = format_caa_human(&obj).unwrap();
        assert_eq!(
            out,
            "No CA is allowed to issue wildcard certificates"
        );
    }

    #[test]
    fn format_caa_issuewild_ca() {
        let obj = json!({"issuer_critical": false, "tag": "issuewild", "value": "letsencrypt.org"});
        let out = format_caa_human(&obj).unwrap();
        assert_eq!(
            out,
            "Allow letsencrypt.org to issue wildcard certificates"
        );
    }

    #[test]
    fn format_caa_iodef() {
        let obj = json!({"issuer_critical": false, "tag": "iodef", "value": "mailto:security@example.com"});
        let out = format_caa_human(&obj).unwrap();
        assert_eq!(
            out,
            "Report policy violations to security@example.com"
        );
    }

    #[test]
    fn format_caa_critical() {
        let obj = json!({"issuer_critical": true, "tag": "issue", "value": "letsencrypt.org"});
        let out = format_caa_human(&obj).unwrap();
        assert!(out.ends_with(" (critical)"), "got: {out}");
    }

    #[test]
    fn format_caa_unknown_tag() {
        let obj = json!({"issuer_critical": false, "tag": "contactemail", "value": "admin@example.com"});
        let out = format_caa_human(&obj).unwrap();
        assert_eq!(out, "contactemail admin@example.com");
    }

    // --- MX formatting tests ---

    #[test]
    fn format_mx_basic() {
        let obj = json!({"preference": 10, "exchange": "mail.example.com."});
        let out = format_mx_human(&obj).unwrap();
        assert_eq!(out, "mail.example.com. with priority 10");
    }

    // --- SOA formatting tests ---

    #[test]
    fn format_soa_basic() {
        let obj = json!({
            "mname": "ns1.example.com.",
            "rname": "admin.example.com.",
            "serial": 2024010101u64,
            "refresh": 3600,
            "retry": 900,
            "expire": 604800,
            "minimum": 86400
        });
        let out = format_soa_human(&obj).unwrap();
        assert!(out.contains("Primary NS: ns1.example.com."), "got: {out}");
        assert!(out.contains("Contact: admin@example.com"), "got: {out}");
        assert!(out.contains("Serial: 2024010101"), "got: {out}");
        assert!(out.contains("Refresh: 3600"), "got: {out}");
        assert!(out.contains("Retry: 900"), "got: {out}");
        assert!(out.contains("Expire: 604800"), "got: {out}");
        assert!(out.contains("Minimum TTL: 86400"), "got: {out}");
    }

    // --- enrich integration tests for CAA/MX/SOA ---

    #[test]
    fn enrich_caa_injects_human() {
        let mut value = json!({
            "lookups": {
                "lookups": [{
                    "result": {
                        "Response": {
                            "records": [{
                                "data": {
                                    "CAA": {
                                        "issuer_critical": false,
                                        "tag": "issue",
                                        "value": "letsencrypt.org"
                                    }
                                }
                            }]
                        }
                    }
                }]
            }
        });
        enrich_lookups_json(&mut value, "CAA");
        let caa = &value["lookups"]["lookups"][0]["result"]["Response"]["records"][0]["data"]["CAA"];
        assert_eq!(
            caa["caa_human"],
            "Allow letsencrypt.org to issue certificates"
        );
    }

    #[test]
    fn enrich_mx_injects_human() {
        let mut value = json!({
            "lookups": {
                "lookups": [{
                    "result": {
                        "Response": {
                            "records": [{
                                "data": {
                                    "MX": {
                                        "preference": 10,
                                        "exchange": "mail.example.com."
                                    }
                                }
                            }]
                        }
                    }
                }]
            }
        });
        enrich_lookups_json(&mut value, "MX");
        let mx = &value["lookups"]["lookups"][0]["result"]["Response"]["records"][0]["data"]["MX"];
        assert_eq!(mx["mx_human"], "mail.example.com. with priority 10");
    }

    #[test]
    fn enrich_soa_injects_human() {
        let mut value = json!({
            "lookups": {
                "lookups": [{
                    "result": {
                        "Response": {
                            "records": [{
                                "data": {
                                    "SOA": {
                                        "mname": "ns1.example.com.",
                                        "rname": "admin.example.com.",
                                        "serial": 2024010101u64,
                                        "refresh": 3600,
                                        "retry": 900,
                                        "expire": 604800,
                                        "minimum": 86400
                                    }
                                }
                            }]
                        }
                    }
                }]
            }
        });
        enrich_lookups_json(&mut value, "SOA");
        let soa = &value["lookups"]["lookups"][0]["result"]["Response"]["records"][0]["data"]["SOA"];
        let human = soa["soa_human"].as_str().unwrap();
        assert!(human.contains("Primary NS: ns1.example.com."), "got: {human}");
        assert!(human.contains("Serial: 2024010101"), "got: {human}");
    }
}
