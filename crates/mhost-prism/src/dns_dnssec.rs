//! DNSSEC chain-of-trust walk.
//!
//! Walks the delegation chain from root → TLD → authoritative, querying DNSKEY,
//! DS, and RRSIG records at each level with the DO (DNSSEC OK) bit set. Validates
//! the cryptographic chain of trust: DS→DNSKEY hash binding, RRSIG expiration,
//! and key tag consistency.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::time::{Duration, Instant};

use hickory_proto::dnssec::rdata::{DNSKEY, DNSSECRData, DS, RRSIG};
use hickory_proto::dnssec::{Algorithm, Verifier};
use hickory_proto::rr::{Name, RData, Record, RecordType};
use serde::Serialize;

use crate::dns_raw::{
    DnsRecord, ROOT_SERVERS, build_server_list, parallel_queries, raw_query_dnssec,
    record_to_dns_record, resolve_missing_glue,
};

// ---------------------------------------------------------------------------
// Public data model (serialised into SSE events)
// ---------------------------------------------------------------------------

/// A single zone level in the DNSSEC chain.
#[derive(Debug, Clone, Serialize)]
pub struct ChainLevel {
    pub level: usize,
    pub zone: String,
    pub servers_queried: usize,
    pub dnskey_records: Vec<DnsRecord>,
    pub ds_records: Vec<DnsRecord>,
    pub rrsig_records: Vec<DnsRecord>,
    pub findings: Vec<ChainFinding>,
    pub latency_ms: f64,
    pub is_final: bool,
}

/// A validation finding for a DNSSEC chain level.
#[derive(Debug, Clone, Serialize)]
pub struct ChainFinding {
    pub severity: String, // "ok", "warning", "failed"
    pub message: String,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Parse and validate a domain name string.
pub fn parse_name(domain: &str) -> Result<Name, String> {
    Name::from_str(domain).map_err(|e| format!("invalid domain: {e}"))
}

/// Walk the DNSSEC chain from root to authoritative for `name`.
///
/// At each delegation level, queries DNSKEY (at the zone's own servers) and
/// DS (at the parent zone's servers, for non-root zones). Collects RRSIG
/// records returned alongside DNSKEY. Validates the cryptographic chain of
/// trust at each level.
pub async fn walk_chain(
    name: Name,
    max_hops: usize,
    query_timeout: Duration,
) -> Vec<ChainLevel> {
    // Start with all IPv4 root servers.
    let mut current_servers: Vec<(SocketAddr, Option<String>)> = ROOT_SERVERS
        .iter()
        .map(|ip| (SocketAddr::new(IpAddr::V4(*ip), 53), None))
        .collect();

    let mut levels = Vec::new();
    // Track parent servers for DS queries at child zones.
    let mut parent_servers: Vec<SocketAddr> = Vec::new();

    // Build the zone labels we need to visit: root → TLD → ... → target
    let zone_labels = build_zone_labels(&name);

    for (idx, target_zone) in zone_labels.iter().enumerate() {
        let level = idx + 1;
        if level > max_hops {
            break;
        }

        if current_servers.is_empty() {
            tracing::warn!(level, zone = %target_zone, "no servers for DNSSEC chain level");
            break;
        }

        let server_addrs: Vec<SocketAddr> =
            current_servers.iter().map(|(addr, _)| *addr).collect();
        let is_final = idx == zone_labels.len() - 1;

        tracing::debug!(
            level,
            servers = server_addrs.len(),
            zone = %target_zone,
            "DNSSEC chain level"
        );

        let start = Instant::now();

        let zone_name = Name::from_str(target_zone)
            .unwrap_or_else(|_| Name::root());

        // Query DNSKEY at this zone's servers (with DO bit).
        let dnskey_raw =
            query_record_type_dnssec(&server_addrs, &zone_name, RecordType::DNSKEY, query_timeout)
                .await;

        // Query DS at parent servers (not for root).
        let ds_response = if !parent_servers.is_empty() && target_zone != "." {
            query_record_type_dnssec(
                &parent_servers,
                &zone_name,
                RecordType::DS,
                query_timeout,
            )
            .await
        } else {
            Vec::new()
        };

        // Separate RRSIG records from the DNSKEY response.
        let mut rrsig_raw = Vec::new();
        let mut dnskey_only = Vec::new();
        for rec in &dnskey_raw {
            if rec.record_type() == RecordType::RRSIG {
                rrsig_raw.push(rec.clone());
            } else {
                dnskey_only.push(rec.clone());
            }
        }

        // Separate RRSIG records from the DS response.
        let mut ds_raw = Vec::new();
        for rec in &ds_response {
            if rec.record_type() == RecordType::RRSIG {
                // DS RRSIG — skip (we don't validate these yet)
            } else {
                ds_raw.push(rec.clone());
            }
        }

        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

        // Validate the cryptographic chain at this level.
        let findings = analyze_level(
            target_zone,
            &zone_name,
            &dnskey_only,
            &ds_raw,
            &rrsig_raw,
            level == 1, // is_root
        );

        // Convert raw records to DnsRecord for frontend display.
        let dnskey_records: Vec<DnsRecord> = dnskey_only.iter().map(record_to_dns_record).collect();
        let ds_records: Vec<DnsRecord> = ds_raw.iter().map(record_to_dns_record).collect();
        let rrsig_records: Vec<DnsRecord> = rrsig_raw.iter().map(record_to_dns_record).collect();

        levels.push(ChainLevel {
            level,
            zone: target_zone.clone(),
            servers_queried: server_addrs.len(),
            dnskey_records,
            ds_records,
            rrsig_records,
            findings,
            latency_ms,
            is_final,
        });

        if is_final {
            break;
        }

        // Follow delegation to next zone.
        let next_zone = zone_labels.get(idx + 1);
        if let Some(next) = next_zone {
            let next_name = Name::from_str(next).unwrap_or_else(|_| Name::root());
            parent_servers = server_addrs.clone();

            let ns_results =
                parallel_queries(&server_addrs, &next_name, RecordType::NS, query_timeout).await;

            let mut next_ns: HashMap<String, Vec<IpAddr>> = HashMap::new();
            for rqr in &ns_results {
                if let Ok(response) = &rqr.result {
                    let ns_names = response.referral_ns_names();
                    let glue = response.glue_ips();
                    for ns_name in &ns_names {
                        let ips: Vec<IpAddr> = glue
                            .iter()
                            .filter(|(name, _)| name == ns_name)
                            .map(|(_, ip)| *ip)
                            .collect();
                        let entry = next_ns.entry(ns_name.to_ascii()).or_default();
                        for ip in ips {
                            if !entry.contains(&ip) {
                                entry.push(ip);
                            }
                        }
                    }
                    for record in response.answers() {
                        if record.record_type() == RecordType::NS
                            && let RData::NS(ns) = record.data()
                        {
                            next_ns.entry(ns.0.to_ascii()).or_default();
                        }
                    }
                }
            }

            resolve_missing_glue(&mut next_ns).await;
            current_servers = build_server_list(&next_ns, |ip| ip.is_ipv4());
        }
    }

    levels
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Build the list of zone labels from root to the target domain.
/// e.g. "example.com" → [".", "com.", "example.com."]
fn build_zone_labels(name: &Name) -> Vec<String> {
    let mut labels = vec![".".to_string()];
    let name_str = name.to_ascii();
    let trimmed = name_str.trim_end_matches('.');

    if trimmed.is_empty() {
        return labels;
    }

    let parts: Vec<&str> = trimmed.split('.').collect();
    for i in (0..parts.len()).rev() {
        let zone = parts[i..].join(".") + ".";
        labels.push(zone);
    }

    labels
}

/// Query a specific record type at the given servers with the DO bit set,
/// returning raw hickory-proto Record objects for typed analysis.
async fn query_record_type_dnssec(
    servers: &[SocketAddr],
    name: &Name,
    record_type: RecordType,
    timeout: Duration,
) -> Vec<Record> {
    // Query a subset of servers (no need to query all 13 root servers).
    let query_servers = if servers.len() > 3 {
        &servers[..3]
    } else {
        servers
    };

    let mut records = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for &server in query_servers {
        match raw_query_dnssec(server, name, record_type, timeout).await {
            Ok(response) => {
                for record in response.answers() {
                    // Deduplicate by (record_type, rdata display).
                    let key = format!("{}:{}", record.record_type(), record.data());
                    if seen.insert(key) {
                        records.push(record.clone());
                    }
                }
                if !records.is_empty() {
                    break;
                }
            }
            Err(e) => {
                tracing::debug!(
                    %server,
                    %record_type,
                    error = %e,
                    "DNSSEC query failed, trying next server"
                );
            }
        }
    }

    records
}

// ---------------------------------------------------------------------------
// DNSSEC chain validation
// ---------------------------------------------------------------------------

fn current_unix_time() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as u32
}

#[allow(deprecated)] // RSASHA1 variants are deprecated but we need to name them
fn algorithm_name(algo: Algorithm) -> &'static str {
    match algo {
        Algorithm::RSASHA1 => "RSA/SHA-1",
        Algorithm::RSASHA1NSEC3SHA1 => "RSA/SHA-1-NSEC3",
        Algorithm::RSASHA256 => "RSA/SHA-256",
        Algorithm::RSASHA512 => "RSA/SHA-512",
        Algorithm::ECDSAP256SHA256 => "ECDSA P-256/SHA-256",
        Algorithm::ECDSAP384SHA384 => "ECDSA P-384/SHA-384",
        Algorithm::ED25519 => "Ed25519",
        _ => "unknown",
    }
}

#[allow(deprecated)]
fn is_deprecated_algorithm(algo: Algorithm) -> bool {
    matches!(algo, Algorithm::RSASHA1 | Algorithm::RSASHA1NSEC3SHA1)
}

/// Analyze DNSSEC records at a single zone level using typed hickory-proto
/// records for cryptographic validation.
fn analyze_level(
    zone: &str,
    zone_name: &Name,
    raw_dnskeys: &[Record],
    raw_ds: &[Record],
    raw_rrsigs: &[Record],
    is_root: bool,
) -> Vec<ChainFinding> {
    let mut findings = Vec::new();

    // Extract typed DNSKEY records.
    let dnskeys: Vec<&DNSKEY> = raw_dnskeys
        .iter()
        .filter_map(|r| match r.data() {
            RData::DNSSEC(DNSSECRData::DNSKEY(dk)) => Some(dk),
            _ => None,
        })
        .collect();

    // Extract typed DS records.
    let ds_list: Vec<&DS> = raw_ds
        .iter()
        .filter_map(|r| match r.data() {
            RData::DNSSEC(DNSSECRData::DS(ds)) => Some(ds),
            _ => None,
        })
        .collect();

    // Extract typed RRSIG records (RRSIG derefs to SIG).
    let rrsigs: Vec<&RRSIG> = raw_rrsigs
        .iter()
        .filter_map(|r| match r.data() {
            RData::DNSSEC(DNSSECRData::RRSIG(sig)) => Some(sig),
            _ => None,
        })
        .collect();

    // -- 1. DNSKEY presence ---------------------------------------------------

    if dnskeys.is_empty() {
        findings.push(ChainFinding {
            severity: "failed".into(),
            message: format!("No DNSKEY records found at {zone}"),
        });
        return findings;
    }

    // -- 2. KSK / ZSK checks -------------------------------------------------

    let ksks: Vec<&DNSKEY> = dnskeys
        .iter()
        .copied()
        .filter(|dk| dk.secure_entry_point())
        .collect();
    let zsks: Vec<&DNSKEY> = dnskeys
        .iter()
        .copied()
        .filter(|dk| dk.zone_key() && !dk.secure_entry_point())
        .collect();

    if !ksks.is_empty() {
        let tags: Vec<String> = ksks
            .iter()
            .filter_map(|dk| dk.calculate_key_tag().ok())
            .map(|t: u16| t.to_string())
            .collect();
        let algos: Vec<&str> = ksks.iter().map(|dk| algorithm_name(dk.algorithm())).collect();
        let algo_str = dedup_join(&algos);
        findings.push(ChainFinding {
            severity: "ok".into(),
            message: format!(
                "KSK present (tag {}, {algo_str})",
                tags.join(", ")
            ),
        });
    } else {
        findings.push(ChainFinding {
            severity: "warning".into(),
            message: "No KSK (SEP) found in DNSKEY records".into(),
        });
    }

    if zsks.is_empty() {
        findings.push(ChainFinding {
            severity: "warning".into(),
            message: "No ZSK found in DNSKEY records".into(),
        });
    }

    // -- 3. RRSIG covering DNSKEY ---------------------------------------------

    let dnskey_rrsigs: Vec<&RRSIG> = rrsigs
        .iter()
        .copied()
        .filter(|sig| sig.type_covered() == RecordType::DNSKEY)
        .collect();

    if dnskey_rrsigs.is_empty() {
        findings.push(ChainFinding {
            severity: "failed".into(),
            message: "No RRSIG covering DNSKEY records".into(),
        });
    } else {
        let now = current_unix_time();
        for sig in &dnskey_rrsigs {
            let expiration = sig.sig_expiration().get();
            let inception = sig.sig_inception().get();
            let key_tag = sig.key_tag();

            if now > expiration {
                findings.push(ChainFinding {
                    severity: "failed".into(),
                    message: format!("RRSIG covering DNSKEY (tag {key_tag}) has expired"),
                });
            } else if now < inception {
                findings.push(ChainFinding {
                    severity: "failed".into(),
                    message: format!("RRSIG covering DNSKEY (tag {key_tag}) not yet valid"),
                });
            } else {
                let days_left = (expiration - now) / 86400;
                if days_left <= 7 {
                    findings.push(ChainFinding {
                        severity: "warning".into(),
                        message: format!(
                            "RRSIG covering DNSKEY (tag {key_tag}) expires in {days_left}d"
                        ),
                    });
                } else {
                    findings.push(ChainFinding {
                        severity: "ok".into(),
                        message: format!(
                            "DNSKEY signed (tag {key_tag}, expires in {days_left}d)"
                        ),
                    });
                }
            }

            // Verify the signing key exists in the DNSKEY set.
            let has_signing_key = dnskeys
                .iter()
                .any(|dk| dk.calculate_key_tag().ok() == Some(key_tag));
            if !has_signing_key {
                findings.push(ChainFinding {
                    severity: "warning".into(),
                    message: format!(
                        "RRSIG key tag {key_tag} does not match any DNSKEY at {zone}"
                    ),
                });
            }
        }
    }

    // -- 4. DS → DNSKEY binding (not applicable for root) ---------------------

    if !is_root {
        if ds_list.is_empty() {
            findings.push(ChainFinding {
                severity: "failed".into(),
                message: format!("No DS records found in parent zone for {zone}"),
            });
        } else {
            let mut any_verified = false;
            for ds in &ds_list {
                let ds_tag = ds.key_tag();
                // Cryptographic verification: hash the DNSKEY and compare to DS digest.
                let matched = dnskeys
                    .iter()
                    .any(|dk| ds.covers(zone_name, dk).unwrap_or(false));

                if matched {
                    any_verified = true;
                    findings.push(ChainFinding {
                        severity: "ok".into(),
                        message: format!(
                            "DS tag {ds_tag} verified against DNSKEY"
                        ),
                    });
                } else {
                    findings.push(ChainFinding {
                        severity: "failed".into(),
                        message: format!(
                            "DS tag {ds_tag} has no matching DNSKEY: chain of trust is broken"
                        ),
                    });
                }
            }
            if !any_verified {
                findings.push(ChainFinding {
                    severity: "failed".into(),
                    message: "No DS record could be verified against any DNSKEY".into(),
                });
            }
        }
    }

    // -- 5. Algorithm strength warnings ---------------------------------------

    for &dk in &dnskeys {
        let algo = dk.algorithm();
        if is_deprecated_algorithm(algo)
            && let Ok(tag) = dk.calculate_key_tag()
        {
            findings.push(ChainFinding {
                severity: "warning".into(),
                message: format!(
                    "Deprecated algorithm {} in DNSKEY tag {tag}",
                    algorithm_name(algo)
                ),
            });
        }
    }

    findings
}

/// Deduplicate and join string slices.
fn dedup_join(items: &[&str]) -> String {
    let mut seen = Vec::new();
    for item in items {
        if !seen.contains(item) {
            seen.push(item);
        }
    }
    seen.join(", ")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_zone_labels_simple() {
        let name = Name::from_str("example.com.").unwrap();
        let labels = build_zone_labels(&name);
        assert_eq!(labels, vec![".", "com.", "example.com."]);
    }

    #[test]
    fn build_zone_labels_subdomain() {
        let name = Name::from_str("www.example.com.").unwrap();
        let labels = build_zone_labels(&name);
        assert_eq!(labels, vec![".", "com.", "example.com.", "www.example.com."]);
    }

    #[test]
    fn build_zone_labels_root() {
        let name = Name::root();
        let labels = build_zone_labels(&name);
        assert_eq!(labels, vec!["."]);
    }

    #[test]
    fn analyze_level_no_dnskey() {
        let zone_name = Name::from_str("example.com.").unwrap();
        let findings = analyze_level("example.com.", &zone_name, &[], &[], &[], false);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, "failed");
        assert!(findings[0].message.contains("No DNSKEY"));
    }

    use hickory_proto::dnssec::PublicKeyBuf;

    fn make_dnskey(zone_key: bool, sep: bool, algo: Algorithm, key_bytes: Vec<u8>) -> DNSKEY {
        DNSKEY::new(zone_key, sep, false, PublicKeyBuf::new(key_bytes, algo))
    }

    #[test]
    fn analyze_level_root_no_ds_check() {
        let dnskey = make_dnskey(true, true, Algorithm::RSASHA256, vec![1, 2, 3, 4]);
        let record = Record::from_rdata(
            Name::root(),
            3600,
            RData::DNSSEC(DNSSECRData::DNSKEY(dnskey)),
        );

        let zone_name = Name::root();
        let findings = analyze_level(".", &zone_name, &[record], &[], &[], true);
        assert!(
            !findings.iter().any(|f| f.message.contains("No DS")),
            "root should not check for DS: {findings:?}"
        );
    }

    #[test]
    fn analyze_level_ds_no_matching_dnskey() {
        let dnskey = make_dnskey(true, true, Algorithm::ECDSAP256SHA256, vec![99, 99, 99, 99]);
        let dnskey_record = Record::from_rdata(
            Name::from_str("example.com.").unwrap(),
            3600,
            RData::DNSSEC(DNSSECRData::DNSKEY(dnskey)),
        );

        // DS with a key_tag that won't match the DNSKEY above.
        let ds = DS::new(
            42069,
            Algorithm::ECDSAP256SHA256,
            hickory_proto::dnssec::DigestType::SHA256,
            vec![0xDE, 0xAD, 0xBE, 0xEF],
        );
        let ds_record = Record::from_rdata(
            Name::from_str("example.com.").unwrap(),
            3600,
            RData::DNSSEC(DNSSECRData::DS(ds)),
        );

        let zone_name = Name::from_str("example.com.").unwrap();
        let findings = analyze_level(
            "example.com.",
            &zone_name,
            &[dnskey_record],
            &[ds_record],
            &[],
            false,
        );
        let has_broken = findings
            .iter()
            .any(|f| f.severity == "failed" && f.message.contains("no matching DNSKEY"));
        assert!(has_broken, "should detect DS/DNSKEY mismatch: {findings:?}");
    }
}
