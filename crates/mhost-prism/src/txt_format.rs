//! TXT record human-readable decoding.
//!
//! Provides [`format_txt`] for producing a human-readable string from a TXT
//! record, and [`enrich_lookups_json`] for injecting `txt_string` and
//! `txt_human` fields into a serialized `BatchEvent` JSON value.

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
                format!("  policy: {}", dmarc.policy()),
            ];
            if let Some(sp) = dmarc.subdomain_policy() {
                lines.push(format!("  subdomain policy: {sp}"));
            }
            if let Some(rua) = dmarc.rua() {
                lines.push(format!("  rua: {rua}"));
            }
            if let Some(ruf) = dmarc.ruf() {
                lines.push(format!("  ruf: {ruf}"));
            }
            if let Some(adkim) = dmarc.adkim() {
                lines.push(format!("  dkim alignment: {adkim}"));
            }
            if let Some(aspf) = dmarc.aspf() {
                lines.push(format!("  spf alignment: {aspf}"));
            }
            if let Some(pct) = dmarc.pct() {
                lines.push(format!("  pct: {pct}"));
            }
            if let Some(fo) = dmarc.fo() {
                lines.push(format!("  failure options: {fo}"));
            }
            if let Some(ri) = dmarc.ri() {
                lines.push(format!("  report interval: {ri}"));
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

/// Walk a serialized `BatchEvent` JSON value and inject `txt_string` and
/// `txt_human` fields into every TXT record object found within
/// `lookups.lookups[*].result.Response.records[*].data.TXT`.
///
/// Only acts when `record_type` is `"TXT"` or `"_dmarc"` (check endpoint uses
/// `"_dmarc"` as the label for the DMARC TXT lookup).
pub fn enrich_lookups_json(value: &mut serde_json::Value, record_type: &str) {
    if record_type != "TXT" && record_type != "_dmarc" {
        return;
    }

    // Count inner lookups first to iterate by index, avoiding nested &mut borrows.
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
            // Read txt_data without a mutable borrow first.
            let txt_string = {
                let txt_data = &value["lookups"]["lookups"][li]["result"]["Response"]["records"][ri]
                    ["data"]["TXT"]["txt_data"];
                let chunks = match txt_data.as_array() {
                    Some(arr) => arr,
                    None => continue,
                };
                // Decode each chunk (array of u8) as UTF-8 and join.
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

            // Build a TXT struct from the decoded string to run format_txt.
            let txt = TXT::new(vec![txt_string.clone()]);
            let txt_human = format_txt(&txt);

            // Now mutably borrow to insert.
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
        assert!(out.contains("policy: reject"), "got: {out}");
        assert!(out.contains("rua: mailto:dmarc@example.com"), "got: {out}");
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
    fn enrich_noop_for_non_txt() {
        let mut value = json!({
            "lookups": {
                "lookups": [{
                    "result": {
                        "Response": {
                            "records": [{
                                "data": { "TXT": { "txt_data": [[104, 101, 108, 108, 111]] } }
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
}
