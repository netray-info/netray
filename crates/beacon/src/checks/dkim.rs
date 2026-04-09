use std::sync::OnceLock;

use futures::future::join_all;

use crate::dns::DnsResolver;
use crate::quality::{Category, CheckResult, SubCheck, Verdict};

static PROVIDER_MAP: OnceLock<Vec<(&'static str, Vec<&'static str>)>> = OnceLock::new();

fn provider_map() -> &'static Vec<(&'static str, Vec<&'static str>)> {
    PROVIDER_MAP.get_or_init(|| {
        vec![
            ("google.com", vec!["google"]),
            ("googlemail.com", vec!["google"]),
            ("outlook.com", vec!["selector1", "selector2"]),
            ("protection.outlook.com", vec!["selector1", "selector2"]),
            ("amazonses.com", vec!["amazonses"]),
            ("pphosted.com", vec!["proofpoint"]),
            ("messagelabs.com", vec!["mimecast"]),
        ]
    })
}

/// Derive DKIM selectors from MX hostnames using the provider map.
fn provider_selectors(mx_hosts: &[String]) -> Vec<String> {
    let map = provider_map();
    let mut selectors = Vec::new();

    for host in mx_hosts {
        let host_lower = host.to_lowercase();
        for (suffix, sels) in map {
            if host_lower.ends_with(&format!(".{}", suffix)) || host_lower == *suffix {
                for sel in sels {
                    let s = sel.to_string();
                    if !selectors.contains(&s) {
                        selectors.push(s);
                    }
                }
            }
        }
    }

    selectors
}

/// Check DKIM for the domain.
/// Returns (CheckResult, dkim_found).
pub async fn check_dkim(
    domain: &str,
    mx_hosts: &[String],
    user_selectors: &[String],
    max_user_selectors: usize,
    resolver: &DnsResolver,
) -> (CheckResult, bool) {
    let mut sub_checks = Vec::new();

    // Build candidate selector list
    let mut candidates: Vec<(String, SelectorSource)> = Vec::new();

    // User-supplied selectors (capped)
    for sel in user_selectors.iter().take(max_user_selectors) {
        if !sel.is_empty() {
            candidates.push((sel.clone(), SelectorSource::User));
        }
    }

    // Provider-mapped selectors
    for sel in provider_selectors(mx_hosts) {
        if !candidates.iter().any(|(s, _)| s == &sel) {
            candidates.push((sel, SelectorSource::Provider));
        }
    }

    // Default selector
    if !candidates.iter().any(|(s, _)| s == "default") {
        candidates.push(("default".to_string(), SelectorSource::Default));
    }

    // Parallelize all selector lookups
    let lookup_futures = candidates.iter().map(|(selector, source)| {
        let name = format!("{}._domainkey.{}", selector, domain);
        async move {
            // Follow CNAME chains up to 5 hops
            let mut current_name = name.clone();
            let mut hops = 0;
            loop {
                let cnames = resolver.lookup_cname(&current_name).await;
                if cnames.is_empty() {
                    break;
                }
                hops += 1;
                if hops > 5 {
                    break;
                }
                current_name = cnames[0].trim_end_matches('.').to_string();
            }
            let txt_records = if hops > 5 {
                Vec::new()
            } else {
                resolver.lookup_txt(&current_name).await
            };
            (selector, source, hops, txt_records)
        }
    });

    let results = join_all(lookup_futures).await;

    let mut found_any = false;

    for (selector, source, hops, txt_records) in results {
        if hops > 5 {
            sub_checks.push(SubCheck {
                name: "cname_loop".to_string(),
                verdict: Verdict::Fail,
                detail: "CNAME chain too deep (>5)".to_string(),
            });
            continue;
        }

        if txt_records.is_empty() {
            // NXDOMAIN handling
            if matches!(source, SelectorSource::User) {
                sub_checks.push(SubCheck {
                    name: "selector_not_found".to_string(),
                    verdict: Verdict::Info,
                    detail: format!("selector '{}' not found", selector),
                });
            }
            // Provider-mapped and default selectors: silently omit
            continue;
        }

        found_any = true;

        // Parse DKIM key record
        for record in &txt_records {
            let tags = parse_dkim_tags(record);

            // Check for revoked key
            if let Some(p_val) = tags.get("p") {
                if p_val.is_empty() {
                    sub_checks.push(SubCheck {
                        name: "key_revoked".to_string(),
                        verdict: Verdict::Fail,
                        detail: format!("selector '{}': key revoked (p= empty)", selector),
                    });
                    continue;
                }

                // Parse key type
                let key_type = tags.get("k").map(|s| s.as_str()).unwrap_or("rsa");

                if key_type == "ed25519" {
                    sub_checks.push(SubCheck {
                        name: "ed25519_key".to_string(),
                        verdict: Verdict::Pass,
                        detail: format!("selector '{}': Ed25519 key", selector),
                    });
                } else {
                    // RSA: parse key size via x509-parser
                    match check_rsa_key_size(p_val) {
                        Some(bits) if bits < 1024 => {
                            sub_checks.push(SubCheck {
                                name: "weak_rsa_key".to_string(),
                                verdict: Verdict::Fail,
                                detail: format!(
                                    "selector '{}': RSA {} bits < 1024",
                                    selector, bits
                                ),
                            });
                        }
                        Some(bits) if bits < 2048 => {
                            sub_checks.push(SubCheck {
                                name: "short_rsa_key".to_string(),
                                verdict: Verdict::Warn,
                                detail: format!(
                                    "selector '{}': RSA {} bits; recommend >= 2048",
                                    selector, bits
                                ),
                            });
                        }
                        Some(bits) => {
                            sub_checks.push(SubCheck {
                                name: "rsa_key_ok".to_string(),
                                verdict: Verdict::Pass,
                                detail: format!(
                                    "selector '{}': RSA {} bits",
                                    selector, bits
                                ),
                            });
                        }
                        None => {
                            sub_checks.push(SubCheck {
                                name: "key_parse_error".to_string(),
                                verdict: Verdict::Info,
                                detail: format!(
                                    "selector '{}': could not parse key",
                                    selector
                                ),
                            });
                        }
                    }
                }
            }
        }
    }

    if !found_any {
        sub_checks.push(SubCheck {
            name: "no_dkim".to_string(),
            verdict: Verdict::Info,
            detail: "no DKIM keys found for any selector".to_string(),
        });
    }

    let detail = if found_any {
        "DKIM key(s) found".to_string()
    } else {
        "No DKIM keys found".to_string()
    };

    let result = CheckResult::new(Category::Dkim, sub_checks, detail);
    (result, found_any)
}

/// Parse DKIM TXT record tags into key-value pairs.
fn parse_dkim_tags(record: &str) -> std::collections::HashMap<String, String> {
    let mut tags = std::collections::HashMap::new();
    for part in record.split(';') {
        let part = part.trim();
        if let Some((key, value)) = part.split_once('=') {
            tags.insert(
                key.trim().to_lowercase(),
                value.trim().replace(' ', ""),
            );
        }
    }
    tags
}

/// Check RSA key size by parsing the SPKI from base64-decoded p= value.
fn check_rsa_key_size(p_value: &str) -> Option<usize> {
    use base64::Engine;
    use x509_parser::prelude::FromDer;
    use x509_parser::public_key::PublicKey;

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(p_value.replace(' ', ""))
        .ok()?;

    let (_, spki) =
        x509_parser::prelude::SubjectPublicKeyInfo::<'_>::from_der(&decoded).ok()?;
    match spki.parsed() {
        Ok(PublicKey::RSA(rsa)) => Some(rsa.key_size()),
        _ => None,
    }
}

#[derive(Debug)]
enum SelectorSource {
    User,
    Provider,
    Default,
}
