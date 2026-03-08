//! Record human-readable formatting.
//!
//! Provides two layers of human-readable formatting for DNS record types:
//! - `*_human`: concise, formatted display — readable field names, one info per
//!   line, decoded values (always shown in table cells)
//! - `*_explain`: educational explanations of what fields mean and why they
//!   matter (shown on demand via the "explain" toggle)
//!
//! [`enrich_lookups_json`] injects both fields into serialized `BatchEvent`
//! JSON values.

use mhost::resources::rdata::TXT;
use mhost::resources::rdata::parsed_txt::{Mechanism, Modifier, ParsedTxt, Qualifier, Word};

// ---------------------------------------------------------------------------
// TXT — SPF, DMARC, and other parsed types
// ---------------------------------------------------------------------------

/// Decode a TXT record to a concise human-readable string.
pub fn format_txt_human(txt: &TXT) -> String {
    let text = txt.as_string();
    match ParsedTxt::from_str(&text) {
        Ok(ParsedTxt::Spf(spf)) => {
            let mut lines = vec![format!("SPF version {}", spf.version())];
            for word in spf.words() {
                match word {
                    Word::Word(q, mechanism) => {
                        let q_prefix = match q {
                            Qualifier::Pass => "+",
                            Qualifier::Neutral => "?",
                            Qualifier::Softfail => "~",
                            Qualifier::Fail => "-",
                        };
                        let mech_str = match mechanism {
                            Mechanism::All => "all".to_string(),
                            Mechanism::A {
                                domain_spec,
                                cidr_len,
                            } => {
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
                            Mechanism::MX {
                                domain_spec,
                                cidr_len,
                            } => {
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
                        lines.push(format!("{q_prefix}{mech_str}"));
                    }
                    Word::Modifier(modifier) => match modifier {
                        Modifier::Redirect(d) => lines.push(format!("redirect={d}")),
                        Modifier::Exp(d) => lines.push(format!("exp={d}")),
                    },
                }
            }
            lines.join("\n")
        }
        Ok(ParsedTxt::Dmarc(dmarc)) => {
            let mut lines = vec![format!("version {}", dmarc.version())];
            lines.push(format!("Policy: {}", dmarc.policy()));
            if let Some(sp) = dmarc.subdomain_policy() {
                lines.push(format!("Subdomain policy: {sp}"));
            }
            if let Some(rua) = dmarc.rua() {
                lines.push(format!("Aggregate reports: {}", strip_uri_scheme(rua)));
            }
            if let Some(ruf) = dmarc.ruf() {
                lines.push(format!("Forensic reports: {}", strip_uri_scheme(ruf)));
            }
            if let Some(adkim) = dmarc.adkim() {
                let align = match adkim {
                    "r" => "relaxed",
                    "s" => "strict",
                    other => other,
                };
                lines.push(format!("DKIM alignment: {align}"));
            }
            if let Some(aspf) = dmarc.aspf() {
                let align = match aspf {
                    "r" => "relaxed",
                    "s" => "strict",
                    other => other,
                };
                lines.push(format!("SPF alignment: {align}"));
            }
            if let Some(pct) = dmarc.pct() {
                lines.push(format!("Apply to: {pct}%"));
            }
            if let Some(fo) = dmarc.fo() {
                lines.push(format!("Failure reporting: {fo}"));
            }
            if let Some(ri) = dmarc.ri() {
                lines.push(format!("Report interval: {ri}s"));
            }
            lines.join("\n")
        }
        Ok(ParsedTxt::MtaSts(mta_sts)) => {
            format!(
                "MTA-STS version {}\nid: {}",
                mta_sts.version(),
                mta_sts.id()
            )
        }
        Ok(ParsedTxt::TlsRpt(tls_rpt)) => {
            format!(
                "TLS-RPT version {}\nrua: {}",
                tls_rpt.version(),
                tls_rpt.rua()
            )
        }
        Ok(ParsedTxt::Bimi(bimi)) => {
            let mut lines = vec![format!("BIMI version {}", bimi.version())];
            if let Some(logo) = bimi.logo() {
                lines.push(format!("Logo: {logo}"));
            }
            if let Some(authority) = bimi.authority() {
                lines.push(format!("Authority: {authority}"));
            }
            lines.join("\n")
        }
        Ok(ParsedTxt::DomainVerification(dv)) => {
            format!(
                "Domain verification: {}\nScope: {}\nID: {}",
                dv.verifier(),
                dv.scope(),
                dv.id()
            )
        }
        Err(_) => text,
    }
}

/// Educational explanation for a TXT record.
pub fn format_txt_explain(txt: &TXT) -> Option<String> {
    let text = txt.as_string();
    match ParsedTxt::from_str(&text) {
        Ok(ParsedTxt::Spf(spf)) => {
            let mut lines = vec![
                "Sender Policy Framework — defines which servers may send email for this domain.".to_string(),
            ];
            for word in spf.words() {
                if let Word::Word(q, mechanism) = word {
                    let q_desc = match q {
                        Qualifier::Pass => "pass: accept the message",
                        Qualifier::Neutral => "neutral: no opinion",
                        Qualifier::Softfail => "softfail: accept but mark suspicious",
                        Qualifier::Fail => "fail: reject the message",
                    };
                    let mech_desc = match mechanism {
                        Mechanism::All => Some("\"all\" is a catch-all for any sender not matched above."),
                        Mechanism::Include(_) => Some("\"include\" delegates SPF checking to another domain's SPF record."),
                        Mechanism::A { .. } => Some("\"a\" allows the domain's own A/AAAA addresses to send."),
                        Mechanism::MX { .. } => Some("\"mx\" allows the domain's mail servers to send."),
                        Mechanism::IPv4(_) => Some("\"ip4\" allows a specific IPv4 address or range."),
                        Mechanism::IPv6(_) => Some("\"ip6\" allows a specific IPv6 address or range."),
                        Mechanism::PTR(_) => Some("\"ptr\" checks reverse DNS (deprecated, slow)."),
                        Mechanism::Exists(_) => Some("\"exists\" checks if a domain resolves (macro-based)."),
                    };
                    lines.push(format!("Qualifier \"{q_desc}\"."));
                    if let Some(desc) = mech_desc {
                        lines.push(desc.to_string());
                    }
                } else if let Word::Modifier(modifier) = word {
                    match modifier {
                        Modifier::Redirect(_) => lines.push("\"redirect\" replaces this SPF with another domain's policy entirely.".to_string()),
                        Modifier::Exp(_) => lines.push("\"exp\" provides a custom rejection message shown to senders.".to_string()),
                    }
                }
            }
            // Deduplicate lines (multiple mechanisms of same type would repeat explanations)
            lines.dedup();
            Some(lines.join("\n"))
        }
        Ok(ParsedTxt::Dmarc(dmarc)) => {
            let mut lines = vec![
                "Domain-based Message Authentication, Reporting and Conformance.".to_string(),
            ];
            let policy_explain = match dmarc.policy() {
                "none" => "Policy \"none\" means monitor only — no enforcement action is taken.",
                "quarantine" => "Policy \"quarantine\" means mark as spam if authentication checks fail.",
                "reject" => "Policy \"reject\" means block delivery if authentication checks fail.",
                _ => "",
            };
            if !policy_explain.is_empty() {
                lines.push(policy_explain.to_string());
            }
            if dmarc.adkim().is_some() || dmarc.aspf().is_some() {
                lines.push("Strict alignment requires an exact domain match; relaxed allows subdomains.".to_string());
            }
            Some(lines.join("\n"))
        }
        Ok(ParsedTxt::MtaSts(_)) => Some("MTA-STS (Mail Transfer Agent Strict Transport Security) enforces TLS for inbound email.".to_string()),
        Ok(ParsedTxt::TlsRpt(_)) => Some("TLS-RPT defines where to send reports about TLS connection failures for email delivery.".to_string()),
        Ok(ParsedTxt::Bimi(_)) => Some("BIMI (Brand Indicators for Message Identification) displays a brand logo next to authenticated emails.".to_string()),
        Ok(ParsedTxt::DomainVerification(dv)) => Some(format!("Proves domain ownership to {} by publishing a verification token in DNS.", dv.verifier())),
        Err(_) => None,
    }
}

/// Strip common URI schemes (`mailto:`, `https://`) for cleaner display.
fn strip_uri_scheme(s: &str) -> &str {
    s.strip_prefix("mailto:")
        .or_else(|| s.strip_prefix("https://"))
        .unwrap_or(s)
}

// ---------------------------------------------------------------------------
// CAA
// ---------------------------------------------------------------------------

/// Format a CAA record: readable tag names, clear policy.
pub fn format_caa_human(obj: &serde_json::Value) -> Option<String> {
    let tag = obj.get("tag")?.as_str()?;
    let value = obj.get("value")?.as_str().unwrap_or("");
    let critical = obj
        .get("issuer_critical")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let tag_label = match tag {
        "issue" => "issue",
        "issuewild" => "issue wildcard",
        "iodef" => "violation report",
        t => t,
    };

    let display_value = if tag == "iodef" {
        strip_uri_scheme(value).to_string()
    } else if value.is_empty() {
        "(none — no CA allowed)".to_string()
    } else {
        value.to_string()
    };

    let suffix = if critical { " (critical)" } else { "" };
    Some(format!("{tag_label}: {display_value}{suffix}"))
}

/// Educational explanation for a CAA record.
pub fn format_caa_explain(_obj: &serde_json::Value) -> Option<String> {
    Some("Certification Authority Authorization — controls which CAs may issue certificates for this domain.\n\
          \"issue\" restricts regular certificates. \"issue wildcard\" restricts wildcard certificates.\n\
          \"violation report\" specifies where to send reports when a CA violates the policy.\n\
          \"critical\" means CAs that don't understand this tag must refuse to issue.".to_string())
}

// ---------------------------------------------------------------------------
// MX
// ---------------------------------------------------------------------------

/// Format an MX record: exchange with priority, natural language.
pub fn format_mx_human(obj: &serde_json::Value) -> Option<String> {
    let preference = obj.get("preference")?.as_u64()?;
    let exchange = obj.get("exchange")?.as_str()?;
    if exchange == "." && preference == 0 {
        return Some("Null MX — does not accept email".to_string());
    }
    Some(format!("{exchange} with priority {preference}"))
}

/// Educational explanation for an MX record.
pub fn format_mx_explain(obj: &serde_json::Value) -> Option<String> {
    let preference = obj.get("preference")?.as_u64()?;
    let exchange = obj.get("exchange")?.as_str()?;
    if exchange == "." && preference == 0 {
        return Some(
            "RFC 7505: This domain explicitly declares it does not accept email.\n\
             Mail servers will not attempt delivery."
                .to_string(),
        );
    }
    Some(
        "Lower priority value is tried first.\n\
         Senders deliver mail to this server on port 25 via SMTP."
            .to_string(),
    )
}

// ---------------------------------------------------------------------------
// SOA
// ---------------------------------------------------------------------------

/// Format a SOA record: one field per line, readable labels.
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
        "Primary nameserver: {mname}\n\
         Contact: {contact}\n\
         Serial: {serial}\n\
         Refresh: {refresh}s, Retry: {retry}s, Expire: {expire}s\n\
         Minimum TTL: {minimum}s"
    ))
}

/// Educational explanation for a SOA record.
pub fn format_soa_explain(_obj: &serde_json::Value) -> Option<String> {
    Some(
        "Start of Authority — zone metadata and caching control.\n\
         Serial is incremented on each zone update.\n\
         Refresh: how often secondaries check for updates.\n\
         Retry: retry interval if refresh fails.\n\
         Expire: secondaries discard zone data after this.\n\
         Minimum TTL: negative-cache duration for NXDOMAIN responses."
            .to_string(),
    )
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

// ---------------------------------------------------------------------------
// SVCB / HTTPS
// ---------------------------------------------------------------------------

/// Format an SVCB/HTTPS record: priority, target, decoded params.
pub fn format_svcb_human(obj: &serde_json::Value) -> Option<String> {
    let priority = obj.get("svc_priority")?.as_u64()?;
    let target = obj.get("target_name")?.as_str()?;

    if priority == 0 {
        return Some(format!("Alias mode → {target}"));
    }

    let target_label = if target == "." { "self" } else { target };

    let mut lines = vec![format!("Priority: {priority}, Target: {target_label}")];

    if let Some(params) = obj.get("svc_params").and_then(|v| v.as_array()) {
        for param in params {
            let key = param.get("key").and_then(|k| k.as_str()).unwrap_or("");
            let raw_value = param.get("value").and_then(|v| v.as_str()).unwrap_or("");
            let value = strip_trailing_commas(raw_value);
            let line = match key {
                "alpn" => format!("Protocols: {}", friendly_alpn(value)),
                "port" => format!("Port: {value}"),
                "ipv4hint" => format!("IPv4: {}", value.replace(',', ", ")),
                "ipv6hint" => format!("IPv6: {}", value.replace(',', ", ")),
                "ech" => "ECH: supported".to_string(),
                _ => format!("{key}: {value}"),
            };
            lines.push(line);
        }
    }

    Some(lines.join("\n"))
}

/// Educational explanation for an SVCB/HTTPS record.
pub fn format_svcb_explain(obj: &serde_json::Value) -> Option<String> {
    let priority = obj.get("svc_priority")?.as_u64()?;

    if priority == 0 {
        return Some(
            "Alias mode redirects service binding to another name.\n\
             Clients follow this indirection before connecting."
                .to_string(),
        );
    }

    let mut lines = vec![
        "Service binding — tells clients how to connect directly.".to_string(),
        "Lower priority value is tried first.".to_string(),
    ];

    let target = obj
        .get("target_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if target == "." {
        lines.push("Target \".\" means the queried domain itself.".to_string());
    }

    if let Some(params) = obj.get("svc_params").and_then(|v| v.as_array()) {
        for param in params {
            let key = param.get("key").and_then(|k| k.as_str()).unwrap_or("");
            let explanation = match key {
                "alpn" => {
                    Some("ALPN advertises supported application protocols for the connection.")
                }
                "ipv4hint" | "ipv6hint" => {
                    Some("Address hints let clients skip extra A/AAAA lookups.")
                }
                "ech" => {
                    Some("Encrypted Client Hello hides the server name from network observers.")
                }
                _ => None,
            };
            if let Some(e) = explanation {
                lines.push(e.to_string());
            }
        }
        // Deduplicate (ipv4hint + ipv6hint would repeat)
        lines.dedup();
    }

    Some(lines.join("\n"))
}

// ---------------------------------------------------------------------------
// TLSA
// ---------------------------------------------------------------------------

/// Format a TLSA record: friendly field names, one per line.
pub fn format_tlsa_human(obj: &serde_json::Value) -> Option<String> {
    let cert_usage = obj.get("cert_usage")?.as_str()?;
    let selector = obj.get("selector")?.as_str()?;
    let matching = obj.get("matching")?.as_str()?;

    let usage_label = friendly_tlsa_usage(cert_usage);
    let selector_label = friendly_tlsa_selector(selector);
    let matching_label = friendly_digest_type(matching);

    let cert_hex = format_cert_hex(obj);

    let mut lines = vec![
        format!("Usage: {usage_label}"),
        format!("Selector: {selector_label}"),
        format!("Matching: {matching_label}"),
    ];
    if !cert_hex.is_empty() {
        lines.push(cert_hex);
    }

    Some(lines.join("\n"))
}

/// Educational explanation for a TLSA record.
pub fn format_tlsa_explain(obj: &serde_json::Value) -> Option<String> {
    let cert_usage = obj.get("cert_usage")?.as_str()?;
    let selector = obj.get("selector")?.as_str()?;
    let matching = obj.get("matching")?.as_str()?;

    let usage_explain = match cert_usage {
        "PKIX-TA" | "PkixTa" => {
            "PKIX-TA: must chain to this CA and pass standard PKIX/WebPKI validation."
        }
        "PKIX-EE" | "PkixEe" => {
            "PKIX-EE: server must present this exact certificate, validated via PKIX."
        }
        "DANE-TA" | "DaneTa" => {
            "DANE-TA: must chain to this trust anchor (CA pinning via DNS, bypasses WebPKI)."
        }
        "DANE-EE" | "DaneEe" => {
            "DANE-EE: pins the server's certificate directly via DNS (no CA chain needed)."
        }
        _ => "Unknown certificate usage mode.",
    };
    let selector_explain = match selector {
        "Full" => "Full: matches the entire certificate.",
        "SPKI" | "Spki" => {
            "SPKI: matches the Subject Public Key Info only (survives cert renewal if key is reused)."
        }
        _ => "",
    };
    let matching_explain = match matching {
        "SHA-256" | "Sha256" => "SHA-256: the certificate data is a SHA-256 hash.",
        "SHA-512" | "Sha512" => "SHA-512: the certificate data is a SHA-512 hash.",
        "Full" => "Full: exact match, no hashing.",
        _ => "",
    };

    let mut lines = vec![
        "DANE/TLSA — authenticates TLS certificates via DNS (requires DNSSEC).".to_string(),
        usage_explain.to_string(),
    ];
    if !selector_explain.is_empty() {
        lines.push(selector_explain.to_string());
    }
    if !matching_explain.is_empty() {
        lines.push(matching_explain.to_string());
    }

    Some(lines.join("\n"))
}

fn friendly_tlsa_usage(usage: &str) -> &str {
    match usage {
        "PkixTa" => "PKIX-TA",
        "PkixEe" => "PKIX-EE",
        "DaneTa" => "DANE-TA",
        "DaneEe" => "DANE-EE",
        other => other,
    }
}

fn friendly_tlsa_selector(selector: &str) -> &str {
    match selector {
        "Spki" => "SPKI",
        other => other,
    }
}

fn format_cert_hex(obj: &serde_json::Value) -> String {
    obj.get("cert_data")
        .and_then(|v| v.as_array())
        .map(|bytes| {
            bytes
                .iter()
                .filter_map(|b| b.as_u64())
                .map(|b| format!("{:02x}", b as u8))
                .collect::<String>()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// NAPTR
// ---------------------------------------------------------------------------

/// Format a NAPTR record: readable fields, one per line.
pub fn format_naptr_human(obj: &serde_json::Value) -> Option<String> {
    let order = obj.get("order")?.as_u64()?;
    let preference = obj.get("preference")?.as_u64()?;
    let flags = obj.get("flags").and_then(|v| v.as_str()).unwrap_or("");
    let services = obj.get("services").and_then(|v| v.as_str()).unwrap_or("");
    let regexp = obj.get("regexp").and_then(|v| v.as_str()).unwrap_or("");
    let replacement = obj
        .get("replacement")
        .and_then(|v| v.as_str())
        .unwrap_or(".");

    let flag_label = match flags {
        "u" => "u (terminal)",
        "s" => "s (terminal)",
        "a" => "a (terminal)",
        "p" => "p (protocol-specific)",
        "" => "(non-terminal)",
        _ => flags,
    };

    let mut lines = vec![
        format!("Order: {order}, Pref: {preference}"),
        format!("Flags: {flag_label}"),
        format!("Service: {services}"),
    ];

    if !regexp.is_empty() {
        lines.push(format!("Regexp: {regexp}"));
    } else if replacement != "." {
        lines.push(format!("Replacement: {replacement}"));
    }

    Some(lines.join("\n"))
}

/// Educational explanation for a NAPTR record.
pub fn format_naptr_explain(obj: &serde_json::Value) -> Option<String> {
    let flags = obj.get("flags").and_then(|v| v.as_str()).unwrap_or("");
    let services = obj.get("services").and_then(|v| v.as_str()).unwrap_or("");

    let mut lines = vec![
        "Name Authority Pointer — maps domain names to URIs or service endpoints.".to_string(),
    ];

    let flag_explain = match flags {
        "u" => "Flag \"u\" means URI: the result is a final URI, no further NAPTR lookups needed.",
        "s" => "Flag \"s\" means SRV: the next step is an SRV lookup on the replacement domain.",
        "a" => "Flag \"a\" means address: the next step is an A/AAAA lookup on the replacement.",
        "" => "No flag means non-terminal: further NAPTR lookups follow.",
        _ => "",
    };
    if !flag_explain.is_empty() {
        lines.push(flag_explain.to_string());
    }

    if let Some(target) = services.strip_prefix("E2U+") {
        lines.push(format!(
            "E2U (ENUM) maps telephone numbers to {target} service endpoints."
        ));
    }

    Some(lines.join("\n"))
}

// ---------------------------------------------------------------------------
// DNSKEY
// ---------------------------------------------------------------------------

/// Format a DNSKEY record: role, algorithm, tag.
pub fn format_dnskey_human(obj: &serde_json::Value) -> Option<String> {
    let flags = obj.get("flags")?.as_u64()?;
    let algorithm = obj.get("algorithm")?.as_str()?;
    let key_tag = obj.get("key_tag").and_then(|v| v.as_u64());
    let algo = friendly_algorithm(algorithm);

    let role = if flags & 0x0001 != 0 {
        "KSK (Key Signing Key)"
    } else {
        "ZSK (Zone Signing Key)"
    };

    let mut lines = vec![role.to_string(), format!("Algorithm: {algo}")];

    if let Some(tag) = key_tag {
        lines.push(format!("Key tag: {tag}"));
    }

    if flags & 0x0080 != 0 {
        lines.push("Status: REVOKED".to_string());
    }

    Some(lines.join("\n"))
}

/// Educational explanation for a DNSKEY record.
pub fn format_dnskey_explain(obj: &serde_json::Value) -> Option<String> {
    let flags = obj.get("flags")?.as_u64()?;

    let role_explain = if flags & 0x0001 != 0 {
        "Signs the DNSKEY RRset itself.\n\
         The parent zone's DS record references this key to establish the DNSSEC chain of trust."
    } else {
        "Signs all other record sets in this zone.\n\
         Rotated more frequently than the KSK."
    };

    let mut lines = vec![role_explain.to_string()];
    lines.push("The key tag is a numeric identifier used to match this key to DS records in the parent zone.".to_string());

    if flags & 0x0080 != 0 {
        lines.push("REVOKED: this key has been marked as no longer trusted.".to_string());
    }

    Some(lines.join("\n"))
}

// ---------------------------------------------------------------------------
// DS
// ---------------------------------------------------------------------------

/// Format a DS record: tag, algorithm, digest.
pub fn format_ds_human(obj: &serde_json::Value) -> Option<String> {
    let key_tag = obj.get("key_tag")?.as_u64()?;
    let algorithm = obj.get("algorithm")?.as_str()?;
    let digest_type = obj.get("digest_type")?.as_str()?;
    let algo = friendly_algorithm(algorithm);
    let dt = friendly_digest_type(digest_type);
    let digest = obj.get("digest").and_then(|v| v.as_str()).unwrap_or("");

    let mut lines = vec![
        format!("Delegation Signer"),
        format!("DNSKEY tag: {key_tag}"),
        format!("{algo}, {dt}"),
    ];
    if !digest.is_empty() {
        lines.push(digest.to_string());
    }

    Some(lines.join("\n"))
}

/// Educational explanation for a DS record.
pub fn format_ds_explain(_obj: &serde_json::Value) -> Option<String> {
    Some(
        "Stored in the parent zone to establish the DNSSEC chain of trust.\n\
         Links the parent zone to the child zone's DNSKEY with the matching tag."
            .to_string(),
    )
}

// ---------------------------------------------------------------------------
// NSEC
// ---------------------------------------------------------------------------

/// Format an NSEC record: next name and type list.
pub fn format_nsec_human(obj: &serde_json::Value) -> Option<String> {
    let next = obj.get("next_domain_name")?.as_str()?;
    let types = obj.get("types")?.as_array()?;

    let type_list: Vec<&str> = types.iter().filter_map(|t| t.as_str()).collect();

    Some(format!(
        "Next: {next}\n\
         Types: {}",
        type_list.join(", ")
    ))
}

/// Educational explanation for an NSEC record.
pub fn format_nsec_explain(_obj: &serde_json::Value) -> Option<String> {
    Some(
        "Authenticated denial of existence — proves exactly which record types exist at this name.\n\
         If you queried a type not in this list, this NSEC record proves it does not exist."
            .to_string(),
    )
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Map raw algorithm identifiers to human-friendly names.
///
/// mhost serializes algorithm names in PascalCase (e.g. "EcdsaP256Sha256").
fn friendly_algorithm(algo: &str) -> &str {
    match algo {
        "ECDSAP256SHA256" | "EcdsaP256Sha256" => "ECDSA P-256/SHA-256",
        "ECDSAP384SHA384" | "EcdsaP384Sha384" => "ECDSA P-384/SHA-384",
        "ED25519" | "Ed25519" => "Ed25519",
        "ED448" | "Ed448" => "Ed448",
        "RSASHA1" | "RsaSha1" => "RSA/SHA-1",
        "RSASHA256" | "RSA/SHA-256" | "RsaSha256" => "RSA/SHA-256",
        "RSASHA512" | "RSA/SHA-512" | "RsaSha512" => "RSA/SHA-512",
        "RSASHA1NSEC3SHA1" | "RsaSha1Nsec3Sha1" => "RSA/SHA-1 (NSEC3)",
        other => other,
    }
}

/// Map raw digest type identifiers to human-friendly names.
fn friendly_digest_type(dt: &str) -> &str {
    match dt {
        "SHA-256" | "Sha256" => "SHA-256",
        "SHA-384" | "Sha384" => "SHA-384",
        "SHA-512" | "Sha512" => "SHA-512",
        "SHA-1" | "Sha1" => "SHA-1",
        other => other,
    }
}

/// Strip trailing commas from mhost SvcParam values (e.g. "h2," → "h2").
fn strip_trailing_commas(s: &str) -> &str {
    s.trim_end_matches(',')
}

/// Map ALPN protocol tokens to human-friendly labels.
fn friendly_alpn(alpn: &str) -> String {
    let protocols: Vec<&str> = alpn.split(',').filter(|s| !s.is_empty()).collect();
    protocols
        .iter()
        .map(|p| match *p {
            "h2" => "h2 (HTTP/2)",
            "h3" => "h3 (HTTP/3)",
            "http/1.1" => "http/1.1 (HTTP/1.1)",
            other => other,
        })
        .collect::<Vec<_>>()
        .join(", ")
}

// ---------------------------------------------------------------------------
// JSON enrichment
// ---------------------------------------------------------------------------

/// Walk a serialized `BatchEvent` JSON value and inject human-readable fields
/// into record data objects within
/// `lookups.lookups[*].result.Response.records[*].data.*`.
///
/// Enrichments by record type:
/// - `"TXT"` / `"_dmarc"` → `txt_string`, `txt_human`, `txt_explain`
/// - Other types → `*_human` and `*_explain`
pub fn enrich_lookups_json(value: &mut serde_json::Value, record_type: &str) {
    match record_type {
        "TXT" | "_dmarc" => enrich_txt(value),
        "CAA" => enrich_dual(
            value,
            "CAA",
            format_caa_human,
            format_caa_explain,
            "caa_human",
            "caa_explain",
        ),
        "MX" => enrich_dual(
            value,
            "MX",
            format_mx_human,
            format_mx_explain,
            "mx_human",
            "mx_explain",
        ),
        "SOA" => enrich_dual(
            value,
            "SOA",
            format_soa_human,
            format_soa_explain,
            "soa_human",
            "soa_explain",
        ),
        "SVCB" => enrich_dual(
            value,
            "SVCB",
            format_svcb_human,
            format_svcb_explain,
            "svcb_human",
            "svcb_explain",
        ),
        "HTTPS" => enrich_dual(
            value,
            "HTTPS",
            format_svcb_human,
            format_svcb_explain,
            "svcb_human",
            "svcb_explain",
        ),
        "TLSA" => enrich_dual(
            value,
            "TLSA",
            format_tlsa_human,
            format_tlsa_explain,
            "tlsa_human",
            "tlsa_explain",
        ),
        "NAPTR" => enrich_dual(
            value,
            "NAPTR",
            format_naptr_human,
            format_naptr_explain,
            "naptr_human",
            "naptr_explain",
        ),
        "DNSKEY" => enrich_dual(
            value,
            "DNSKEY",
            format_dnskey_human,
            format_dnskey_explain,
            "dnskey_human",
            "dnskey_explain",
        ),
        "DS" => enrich_dual(
            value,
            "DS",
            format_ds_human,
            format_ds_explain,
            "ds_human",
            "ds_explain",
        ),
        "NSEC" => enrich_dual(
            value,
            "NSEC",
            format_nsec_human,
            format_nsec_explain,
            "nsec_human",
            "nsec_explain",
        ),
        _ => {}
    }
}

/// Enrich TXT records by decoding `txt_data` bytes and injecting `txt_string`,
/// `txt_human`, and `txt_explain`.
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
                let txt_data = &value["lookups"]["lookups"][li]["result"]["Response"]["records"]
                    [ri]["data"]["TXT"]["txt_data"];
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
            let txt_human = format_txt_human(&txt);
            let txt_explain = format_txt_explain(&txt);

            if let Some(obj) = value["lookups"]["lookups"][li]["result"]["Response"]["records"][ri]
                ["data"]["TXT"]
                .as_object_mut()
            {
                obj.insert(
                    "txt_string".to_string(),
                    serde_json::Value::String(txt_string),
                );
                obj.insert(
                    "txt_human".to_string(),
                    serde_json::Value::String(txt_human),
                );
                if let Some(explain) = txt_explain {
                    obj.insert(
                        "txt_explain".to_string(),
                        serde_json::Value::String(explain),
                    );
                }
            }
        }
    }
}

/// Enrich records with both `*_human` and `*_explain` fields.
fn enrich_dual(
    value: &mut serde_json::Value,
    data_key: &str,
    human_fn: fn(&serde_json::Value) -> Option<String>,
    explain_fn: fn(&serde_json::Value) -> Option<String>,
    human_field: &str,
    explain_field: &str,
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
            let (human, explain) = {
                let data = &value["lookups"]["lookups"][li]["result"]["Response"]["records"][ri]["data"]
                    [data_key];
                (human_fn(data), explain_fn(data))
            };

            if let Some(obj) = value["lookups"]["lookups"][li]["result"]["Response"]["records"][ri]
                ["data"][data_key]
                .as_object_mut()
            {
                if let Some(h) = human {
                    obj.insert(human_field.to_string(), serde_json::Value::String(h));
                }
                if let Some(e) = explain {
                    obj.insert(explain_field.to_string(), serde_json::Value::String(e));
                }
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

    // --- TXT human formatting ---

    #[test]
    fn format_spf_human_basic() {
        let txt = make_txt("v=spf1 ip4:192.0.2.0/24 -all");
        let out = format_txt_human(&txt);
        assert!(out.contains("SPF version"), "got: {out}");
        assert!(out.contains("ip4:192.0.2.0/24"), "got: {out}");
        assert!(out.contains("-all"), "got: {out}");
    }

    #[test]
    fn format_spf_explain_present() {
        let txt = make_txt("v=spf1 include:example.com -all");
        let out = format_txt_explain(&txt).unwrap();
        assert!(out.contains("Sender Policy Framework"), "got: {out}");
        assert!(out.contains("include"), "got: {out}");
    }

    #[test]
    fn format_dmarc_human_basic() {
        let txt = make_txt("v=DMARC1; p=reject; rua=mailto:dmarc@example.com");
        let out = format_txt_human(&txt);
        assert!(out.starts_with("version"), "got: {out}");
        assert!(out.contains("Policy: reject"), "got: {out}");
        assert!(out.contains("dmarc@example.com"), "got: {out}");
    }

    #[test]
    fn format_dmarc_human_alignment_readable() {
        let txt = make_txt("v=DMARC1; p=reject; adkim=s; aspf=r");
        let out = format_txt_human(&txt);
        assert!(out.contains("DKIM alignment: strict"), "got: {out}");
        assert!(out.contains("SPF alignment: relaxed"), "got: {out}");
    }

    #[test]
    fn format_dmarc_explain_present() {
        let txt = make_txt("v=DMARC1; p=reject");
        let out = format_txt_explain(&txt).unwrap();
        assert!(
            out.contains("Domain-based Message Authentication"),
            "got: {out}"
        );
        assert!(out.contains("reject"), "got: {out}");
    }

    #[test]
    fn format_mta_sts() {
        let txt = make_txt("v=STSv1; id=20190429T010101");
        let out = format_txt_human(&txt);
        assert!(out.contains("MTA-STS"), "got: {out}");
        assert!(out.contains("20190429T010101"), "got: {out}");
    }

    #[test]
    fn format_tls_rpt() {
        let txt = make_txt("v=TLSRPTv1; rua=mailto:tlsrpt@example.com");
        let out = format_txt_human(&txt);
        assert!(out.contains("TLS-RPT"), "got: {out}");
    }

    #[test]
    fn format_bimi() {
        let txt = make_txt("v=BIMI1; l=https://example.com/logo.svg");
        let out = format_txt_human(&txt);
        assert!(out.contains("BIMI"), "got: {out}");
        assert!(out.contains("logo.svg"), "got: {out}");
    }

    #[test]
    fn format_domain_verification() {
        let txt = make_txt("google-site-verification=abc123");
        let out = format_txt_human(&txt);
        assert!(out.contains("Domain verification"), "got: {out}");
        assert!(out.contains("abc123"), "got: {out}");
    }

    #[test]
    fn format_plain_fallback() {
        let txt = make_txt("some random text that is not parseable");
        let out = format_txt_human(&txt);
        assert_eq!(out, "some random text that is not parseable");
    }

    #[test]
    fn format_plain_no_explain() {
        let txt = make_txt("some random text");
        assert!(format_txt_explain(&txt).is_none());
    }

    // --- CAA formatting ---

    #[test]
    fn format_caa_issue_empty() {
        let obj = json!({"issuer_critical": false, "tag": "issue", "value": ""});
        let out = format_caa_human(&obj).unwrap();
        assert_eq!(out, "issue: (none — no CA allowed)");
    }

    #[test]
    fn format_caa_issue_ca() {
        let obj = json!({"issuer_critical": false, "tag": "issue", "value": "letsencrypt.org"});
        let out = format_caa_human(&obj).unwrap();
        assert_eq!(out, "issue: letsencrypt.org");
    }

    #[test]
    fn format_caa_issuewild() {
        let obj = json!({"issuer_critical": false, "tag": "issuewild", "value": "letsencrypt.org"});
        let out = format_caa_human(&obj).unwrap();
        assert_eq!(out, "issue wildcard: letsencrypt.org");
    }

    #[test]
    fn format_caa_iodef() {
        let obj = json!({"issuer_critical": false, "tag": "iodef", "value": "mailto:security@example.com"});
        let out = format_caa_human(&obj).unwrap();
        assert_eq!(out, "violation report: security@example.com");
    }

    #[test]
    fn format_caa_critical() {
        let obj = json!({"issuer_critical": true, "tag": "issue", "value": "letsencrypt.org"});
        let out = format_caa_human(&obj).unwrap();
        assert!(out.ends_with(" (critical)"), "got: {out}");
    }

    #[test]
    fn format_caa_unknown_tag() {
        let obj =
            json!({"issuer_critical": false, "tag": "contactemail", "value": "admin@example.com"});
        let out = format_caa_human(&obj).unwrap();
        assert_eq!(out, "contactemail: admin@example.com");
    }

    #[test]
    fn format_caa_explain_present() {
        let obj = json!({"issuer_critical": false, "tag": "issue", "value": "x"});
        let out = format_caa_explain(&obj).unwrap();
        assert!(out.contains("Certification Authority"), "got: {out}");
    }

    // --- MX formatting ---

    #[test]
    fn format_mx_basic() {
        let obj = json!({"preference": 10, "exchange": "mail.example.com."});
        let out = format_mx_human(&obj).unwrap();
        assert_eq!(out, "mail.example.com. with priority 10");
    }

    #[test]
    fn format_mx_null() {
        let obj = json!({"preference": 0, "exchange": "."});
        let out = format_mx_human(&obj).unwrap();
        assert_eq!(out, "Null MX — does not accept email");
    }

    #[test]
    fn format_mx_explain_normal() {
        let obj = json!({"preference": 10, "exchange": "mail.example.com."});
        let out = format_mx_explain(&obj).unwrap();
        assert!(out.contains("Lower priority"), "got: {out}");
    }

    #[test]
    fn format_mx_explain_null() {
        let obj = json!({"preference": 0, "exchange": "."});
        let out = format_mx_explain(&obj).unwrap();
        assert!(out.contains("RFC 7505"), "got: {out}");
    }

    // --- SOA formatting ---

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
        assert!(
            out.contains("Primary nameserver: ns1.example.com."),
            "got: {out}"
        );
        assert!(out.contains("Contact: admin@example.com"), "got: {out}");
        assert!(out.contains("Serial: 2024010101"), "got: {out}");
    }

    #[test]
    fn format_soa_explain_present() {
        let obj = json!({
            "mname": "ns1.example.com.", "rname": "admin.example.com.",
            "serial": 1, "refresh": 1, "retry": 1, "expire": 1, "minimum": 1
        });
        let out = format_soa_explain(&obj).unwrap();
        assert!(out.contains("Start of Authority"), "got: {out}");
        assert!(out.contains("Serial is incremented"), "got: {out}");
    }

    // --- SVCB/HTTPS formatting ---

    #[test]
    fn format_svcb_alias() {
        let obj = json!({"svc_priority": 0, "target_name": "cdn.example.com.", "svc_params": []});
        let out = format_svcb_human(&obj).unwrap();
        assert_eq!(out, "Alias mode → cdn.example.com.");
    }

    #[test]
    fn format_svcb_service() {
        let obj = json!({
            "svc_priority": 1,
            "target_name": ".",
            "svc_params": [
                {"key": "alpn", "value": "h2,h3"},
                {"key": "port", "value": "443"}
            ]
        });
        let out = format_svcb_human(&obj).unwrap();
        assert!(out.contains("Priority: 1, Target: self"), "got: {out}");
        assert!(
            out.contains("Protocols: h2 (HTTP/2), h3 (HTTP/3)"),
            "got: {out}"
        );
        assert!(out.contains("Port: 443"), "got: {out}");
    }

    #[test]
    fn format_svcb_explain_alias() {
        let obj = json!({"svc_priority": 0, "target_name": "cdn.example.com.", "svc_params": []});
        let out = format_svcb_explain(&obj).unwrap();
        assert!(out.contains("Alias mode"), "got: {out}");
    }

    #[test]
    fn format_svcb_explain_service() {
        let obj = json!({
            "svc_priority": 1, "target_name": ".",
            "svc_params": [{"key": "alpn", "value": "h2"}]
        });
        let out = format_svcb_explain(&obj).unwrap();
        assert!(out.contains("Service binding"), "got: {out}");
        assert!(out.contains("ALPN"), "got: {out}");
    }

    // --- TLSA formatting ---

    #[test]
    fn format_tlsa_human_dane_ee() {
        let obj = json!({
            "cert_usage": "DaneEe",
            "selector": "Spki",
            "matching": "Sha256",
            "cert_data": [0xab, 0xcd, 0xef]
        });
        let out = format_tlsa_human(&obj).unwrap();
        assert!(out.contains("Usage: DANE-EE"), "got: {out}");
        assert!(out.contains("Selector: SPKI"), "got: {out}");
        assert!(out.contains("Matching: SHA-256"), "got: {out}");
        assert!(out.contains("abcdef"), "got: {out}");
    }

    #[test]
    fn format_tlsa_explain_present() {
        let obj = json!({
            "cert_usage": "DaneEe", "selector": "Spki", "matching": "Sha256", "cert_data": []
        });
        let out = format_tlsa_explain(&obj).unwrap();
        assert!(out.contains("DANE/TLSA"), "got: {out}");
        assert!(out.contains("DANE-EE"), "got: {out}");
    }

    // --- NAPTR formatting ---

    #[test]
    fn format_naptr_enum_sip() {
        let obj = json!({
            "order": 100, "preference": 10,
            "flags": "u", "services": "E2U+sip",
            "regexp": "!^.*$!sip:info@example.com!", "replacement": "."
        });
        let out = format_naptr_human(&obj).unwrap();
        assert!(out.contains("Order: 100, Pref: 10"), "got: {out}");
        assert!(out.contains("Flags: u (terminal)"), "got: {out}");
        assert!(out.contains("Service: E2U+sip"), "got: {out}");
        assert!(out.contains("Regexp:"), "got: {out}");
    }

    #[test]
    fn format_naptr_replacement() {
        let obj = json!({
            "order": 100, "preference": 10,
            "flags": "", "services": "E2U+email",
            "regexp": "", "replacement": "_sip._tcp.example.com."
        });
        let out = format_naptr_human(&obj).unwrap();
        assert!(out.contains("(non-terminal)"), "got: {out}");
        assert!(
            out.contains("Replacement: _sip._tcp.example.com."),
            "got: {out}"
        );
    }

    #[test]
    fn format_naptr_explain_present() {
        let obj = json!({
            "order": 100, "preference": 10,
            "flags": "u", "services": "E2U+sip",
            "regexp": "", "replacement": "."
        });
        let out = format_naptr_explain(&obj).unwrap();
        assert!(out.contains("Name Authority Pointer"), "got: {out}");
        assert!(out.contains("ENUM"), "got: {out}");
    }

    // --- DNSKEY formatting ---

    #[test]
    fn format_dnskey_ksk() {
        let obj = json!({
            "flags": 257, "protocol": 3,
            "algorithm": "ECDSAP256SHA256",
            "public_key": "abc123==", "key_tag": 36315,
            "is_zone_key": true, "is_secure_entry_point": true, "is_revoked": false
        });
        let out = format_dnskey_human(&obj).unwrap();
        assert!(out.contains("KSK (Key Signing Key)"), "got: {out}");
        assert!(out.contains("ECDSA P-256/SHA-256"), "got: {out}");
        assert!(out.contains("Key tag: 36315"), "got: {out}");
    }

    #[test]
    fn format_dnskey_zsk() {
        let obj = json!({
            "flags": 256, "protocol": 3,
            "algorithm": "EcdsaP256Sha256",
            "public_key": "xyz789==", "key_tag": 9976,
            "is_zone_key": true, "is_secure_entry_point": false, "is_revoked": false
        });
        let out = format_dnskey_human(&obj).unwrap();
        assert!(out.contains("ZSK (Zone Signing Key)"), "got: {out}");
        assert!(out.contains("ECDSA P-256/SHA-256"), "got: {out}");
        assert!(out.contains("Key tag: 9976"), "got: {out}");
    }

    #[test]
    fn format_dnskey_revoked() {
        let obj = json!({
            "flags": 385, "protocol": 3,
            "algorithm": "RSA/SHA-256",
            "public_key": "abc==", "key_tag": 1234,
            "is_zone_key": true, "is_secure_entry_point": true, "is_revoked": true
        });
        let out = format_dnskey_human(&obj).unwrap();
        assert!(out.contains("KSK"), "got: {out}");
        assert!(out.contains("REVOKED"), "got: {out}");
    }

    #[test]
    fn format_dnskey_explain_ksk() {
        let obj = json!({"flags": 257, "protocol": 3, "algorithm": "x", "key_tag": 1});
        let out = format_dnskey_explain(&obj).unwrap();
        assert!(out.contains("Signs the DNSKEY RRset"), "got: {out}");
        assert!(out.contains("chain of trust"), "got: {out}");
    }

    #[test]
    fn format_dnskey_explain_zsk() {
        let obj = json!({"flags": 256, "protocol": 3, "algorithm": "x", "key_tag": 1});
        let out = format_dnskey_explain(&obj).unwrap();
        assert!(out.contains("Signs all other record sets"), "got: {out}");
    }

    // --- DS formatting ---

    #[test]
    fn format_ds_basic() {
        let obj = json!({
            "key_tag": 2371,
            "algorithm": "ECDSAP256SHA256",
            "digest_type": "SHA-256",
            "digest": "C9B8EC423E3B80EB"
        });
        let out = format_ds_human(&obj).unwrap();
        assert!(out.contains("Delegation Signer"), "got: {out}");
        assert!(out.contains("DNSKEY tag: 2371"), "got: {out}");
        assert!(out.contains("ECDSA P-256/SHA-256"), "got: {out}");
        assert!(out.contains("C9B8EC423E3B80EB"), "got: {out}");
    }

    #[test]
    fn format_ds_explain_present() {
        let obj = json!({"key_tag": 1, "algorithm": "x", "digest_type": "x", "digest": ""});
        let out = format_ds_explain(&obj).unwrap();
        assert!(out.contains("parent zone"), "got: {out}");
        assert!(out.contains("chain of trust"), "got: {out}");
    }

    // --- NSEC formatting ---

    #[test]
    fn format_nsec_basic() {
        let obj = json!({
            "next_domain_name": "\\000.example.com.",
            "types": ["A", "NS", "SOA", "MX", "TXT", "AAAA", "NSEC"]
        });
        let out = format_nsec_human(&obj).unwrap();
        assert!(out.contains("Next: \\000.example.com."), "got: {out}");
        assert!(
            out.contains("Types: A, NS, SOA, MX, TXT, AAAA, NSEC"),
            "got: {out}"
        );
    }

    #[test]
    fn format_nsec_explain_present() {
        let obj = json!({"next_domain_name": "x.", "types": ["A"]});
        let out = format_nsec_explain(&obj).unwrap();
        assert!(out.contains("denial of existence"), "got: {out}");
    }

    // --- Enrichment integration tests ---

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
        let txt_obj =
            &value["lookups"]["lookups"][0]["result"]["Response"]["records"][0]["data"]["TXT"];
        assert_eq!(txt_obj["txt_string"], "hello");
        assert_eq!(txt_obj["txt_human"], "hello");
    }

    #[test]
    fn enrich_spf_injects_human_and_explain() {
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
                                    "TXT": { "txt_data": [bytes] }
                                }
                            }]
                        }
                    }
                }]
            }
        });
        enrich_lookups_json(&mut value, "TXT");
        let txt_obj =
            &value["lookups"]["lookups"][0]["result"]["Response"]["records"][0]["data"]["TXT"];
        let human = txt_obj["txt_human"].as_str().unwrap();
        assert!(human.contains("SPF version"), "got: {human}");
        let explain = txt_obj["txt_explain"].as_str().unwrap();
        assert!(
            explain.contains("Sender Policy Framework"),
            "got: {explain}"
        );
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
                                    "TXT": { "txt_data": [bytes] }
                                }
                            }]
                        }
                    }
                }]
            }
        });
        enrich_lookups_json(&mut value, "_dmarc");
        let txt_obj =
            &value["lookups"]["lookups"][0]["result"]["Response"]["records"][0]["data"]["TXT"];
        let human = txt_obj["txt_human"].as_str().unwrap();
        assert!(human.starts_with("version"), "got: {human}");
    }

    #[test]
    fn enrich_multi_chunk_txt() {
        let chunk1: Vec<serde_json::Value> = "hello"
            .bytes()
            .map(|b| serde_json::Value::Number(b.into()))
            .collect();
        let chunk2: Vec<serde_json::Value> = " world"
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
                                    "TXT": { "txt_data": [chunk1, chunk2] }
                                }
                            }]
                        }
                    }
                }]
            }
        });
        enrich_lookups_json(&mut value, "TXT");
        let txt_obj =
            &value["lookups"]["lookups"][0]["result"]["Response"]["records"][0]["data"]["TXT"];
        assert_eq!(txt_obj["txt_string"], "hello world");
    }

    #[test]
    fn enrich_caa_injects_human_and_explain() {
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
        let caa =
            &value["lookups"]["lookups"][0]["result"]["Response"]["records"][0]["data"]["CAA"];
        assert_eq!(caa["caa_human"], "issue: letsencrypt.org");
        assert!(
            caa["caa_explain"]
                .as_str()
                .unwrap()
                .contains("Certification Authority")
        );
    }

    #[test]
    fn enrich_mx_injects_human_and_explain() {
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
        let human = mx["mx_human"].as_str().unwrap();
        assert_eq!(human, "mail.example.com. with priority 10");
        assert!(
            mx["mx_explain"]
                .as_str()
                .unwrap()
                .contains("Lower priority")
        );
    }

    #[test]
    fn enrich_svcb_injects_human_and_explain() {
        let mut value = json!({
            "lookups": {
                "lookups": [{
                    "result": {
                        "Response": {
                            "records": [{
                                "data": {
                                    "SVCB": {
                                        "svc_priority": 0,
                                        "target_name": "cdn.example.com.",
                                        "svc_params": []
                                    }
                                }
                            }]
                        }
                    }
                }]
            }
        });
        enrich_lookups_json(&mut value, "SVCB");
        let svcb =
            &value["lookups"]["lookups"][0]["result"]["Response"]["records"][0]["data"]["SVCB"];
        assert!(svcb["svcb_human"].as_str().unwrap().contains("Alias mode"));
        assert!(
            svcb["svcb_explain"]
                .as_str()
                .unwrap()
                .contains("Alias mode")
        );
    }

    #[test]
    fn enrich_dnskey_injects_human_and_explain() {
        let mut value = json!({
            "lookups": {
                "lookups": [{
                    "result": {
                        "Response": {
                            "records": [{
                                "data": {
                                    "DNSKEY": {
                                        "flags": 257, "protocol": 3,
                                        "algorithm": "ECDSAP256SHA256",
                                        "public_key": "abc==", "key_tag": 36315,
                                        "is_zone_key": true, "is_secure_entry_point": true,
                                        "is_revoked": false
                                    }
                                }
                            }]
                        }
                    }
                }]
            }
        });
        enrich_lookups_json(&mut value, "DNSKEY");
        let dnskey =
            &value["lookups"]["lookups"][0]["result"]["Response"]["records"][0]["data"]["DNSKEY"];
        let human = dnskey["dnskey_human"].as_str().unwrap();
        assert!(human.contains("KSK"), "got: {human}");
        let explain = dnskey["dnskey_explain"].as_str().unwrap();
        assert!(explain.contains("chain of trust"), "got: {explain}");
    }

    #[test]
    fn enrich_ds_injects_human_and_explain() {
        let mut value = json!({
            "lookups": {
                "lookups": [{
                    "result": {
                        "Response": {
                            "records": [{
                                "data": {
                                    "DS": {
                                        "key_tag": 2371,
                                        "algorithm": "ECDSAP256SHA256",
                                        "digest_type": "SHA-256",
                                        "digest": "AABBCCDD"
                                    }
                                }
                            }]
                        }
                    }
                }]
            }
        });
        enrich_lookups_json(&mut value, "DS");
        let ds = &value["lookups"]["lookups"][0]["result"]["Response"]["records"][0]["data"]["DS"];
        assert!(ds["ds_human"].as_str().unwrap().contains("2371"));
        assert!(ds["ds_explain"].as_str().unwrap().contains("parent zone"));
    }

    #[test]
    fn enrich_nsec_injects_human_and_explain() {
        let mut value = json!({
            "lookups": {
                "lookups": [{
                    "result": {
                        "Response": {
                            "records": [{
                                "data": {
                                    "NSEC": {
                                        "next_domain_name": "\\000.example.com.",
                                        "types": ["A", "NS", "SOA"]
                                    }
                                }
                            }]
                        }
                    }
                }]
            }
        });
        enrich_lookups_json(&mut value, "NSEC");
        let nsec =
            &value["lookups"]["lookups"][0]["result"]["Response"]["records"][0]["data"]["NSEC"];
        assert!(nsec["nsec_human"].as_str().unwrap().contains("A, NS, SOA"));
        assert!(
            nsec["nsec_explain"]
                .as_str()
                .unwrap()
                .contains("denial of existence")
        );
    }

    #[test]
    fn enrich_soa_injects_human_and_explain() {
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
        let soa =
            &value["lookups"]["lookups"][0]["result"]["Response"]["records"][0]["data"]["SOA"];
        let human = soa["soa_human"].as_str().unwrap();
        assert!(human.contains("Primary nameserver"), "got: {human}");
        let explain = soa["soa_explain"].as_str().unwrap();
        assert!(explain.contains("Start of Authority"), "got: {explain}");
    }

    #[test]
    fn enrich_tlsa_injects_human_and_explain() {
        let mut value = json!({
            "lookups": {
                "lookups": [{
                    "result": {
                        "Response": {
                            "records": [{
                                "data": {
                                    "TLSA": {
                                        "cert_usage": "DANE-EE",
                                        "selector": "SPKI",
                                        "matching": "SHA-256",
                                        "cert_data": [0xab, 0xcd]
                                    }
                                }
                            }]
                        }
                    }
                }]
            }
        });
        enrich_lookups_json(&mut value, "TLSA");
        let tlsa =
            &value["lookups"]["lookups"][0]["result"]["Response"]["records"][0]["data"]["TLSA"];
        assert!(tlsa["tlsa_human"].as_str().unwrap().contains("DANE-EE"));
        assert!(tlsa["tlsa_explain"].as_str().unwrap().contains("DANE/TLSA"));
    }

    #[test]
    fn enrich_naptr_injects_human_and_explain() {
        let mut value = json!({
            "lookups": {
                "lookups": [{
                    "result": {
                        "Response": {
                            "records": [{
                                "data": {
                                    "NAPTR": {
                                        "order": 100, "preference": 10,
                                        "flags": "u", "services": "E2U+sip",
                                        "regexp": "!^.*$!sip:info@example.com!",
                                        "replacement": "."
                                    }
                                }
                            }]
                        }
                    }
                }]
            }
        });
        enrich_lookups_json(&mut value, "NAPTR");
        let naptr =
            &value["lookups"]["lookups"][0]["result"]["Response"]["records"][0]["data"]["NAPTR"];
        assert!(
            naptr["naptr_human"]
                .as_str()
                .unwrap()
                .contains("Order: 100")
        );
        assert!(
            naptr["naptr_explain"]
                .as_str()
                .unwrap()
                .contains("Name Authority Pointer")
        );
    }
}
