//! The legacy configuration table: key paths of the six retired config files, refused on the raw
//! file before deserialization with their destination (specs/features/v2-cutover, requirement 11).

enum Fate {
    /// The value lives at this place now.
    Now(&'static str),
    /// Removed in 0.30.0.
    Removed,
    /// Removed in 0.30.0: enrichment calls the in-process ip module.
    InProcess,
    /// No single destination: the CHANGELOG table gives each service's fate.
    Table,
}

use Fate::{InProcess, Now, Removed, Table};

/// `<m>` in a path matches any one segment and is substituted into the destination.
const ROWS: &[(&str, &str, Fate)] = &[
    (
        "backends.resolve_timeout_ms",
        "lens",
        Now("[engine] resolve_timeout_ms"),
    ),
    (
        "backends.<m>.timeout_ms",
        "lens",
        Now("[modules.<m>] deadline_ms"),
    ),
    ("backends.<m>.url", "lens", Removed),
    ("backends.ip.url", "prism, tlsight", InProcess),
    ("backends.ip.timeout_ms", "prism, tlsight", InProcess),
    ("backends.ip.max_concurrent", "prism, tlsight", InProcess),
    ("backends.ip.cache_ttl_secs", "prism, tlsight", InProcess),
    ("backends.ip.cache_capacity", "prism, tlsight", InProcess),
    ("backends.ip_url", "beacon", InProcess),
    ("backends.timeout_ms", "beacon", InProcess),
    ("modules.dns.backends.ip", "lens", InProcess),
    ("modules.tls.backends.ip", "lens", InProcess),
    ("modules.http.enrichment", "lens", InProcess),
    ("modules.email.backends.ip_url", "lens", InProcess),
    ("cache.enabled", "lens", Now("[admission] cache_enabled")),
    ("cache.enabled", "ifconfig", Removed),
    (
        "cache.ttl_seconds",
        "lens",
        Now("[admission] cache_window_seconds"),
    ),
    ("cache.ttl_secs", "ifconfig", Removed),
    ("cache.max_entries", "ifconfig", Removed),
    (
        "rate_limit.per_ip_per_minute",
        "lens",
        Now("[admission] per_ip_per_minute"),
    ),
    (
        "rate_limit.per_ip_burst",
        "lens",
        Now("[admission] per_ip_burst"),
    ),
    (
        "rate_limit.global_per_minute",
        "lens",
        Now("[admission] global_per_minute"),
    ),
    (
        "rate_limit.global_burst",
        "lens",
        Now("[admission] global_burst"),
    ),
    (
        "rate_limit.per_ip_per_minute",
        "ifconfig",
        Now("[api.ip] per_ip_per_minute"),
    ),
    (
        "rate_limit.per_ip_burst",
        "ifconfig",
        Now("[api.ip] per_ip_burst"),
    ),
    (
        "rate_limit.per_target_per_minute",
        "ifconfig",
        Now("[api.ip] per_target_per_minute"),
    ),
    (
        "rate_limit.per_target_burst",
        "ifconfig",
        Now("[api.ip] per_target_burst"),
    ),
    // Exempted lens's calls to ifconfig; enrichment is in-process now, and `[api.ip]` has no
    // exemption list.
    ("rate_limit.exempt_cidrs", "ifconfig", Removed),
    ("rate_limit.per_ip", "beacon", Removed),
    ("scoring.profile_path", "lens", Now("[engine] profile_path")),
    ("history.enabled", "lens", Now("[store] enabled")),
    ("history.db_path", "lens", Now("[store] db_path")),
    ("snapshots", "lens", Removed),
    ("site", "lens", Removed),
    ("ecosystem", "lens, prism, tlsight, beacon", Removed),
    ("site_name", "prism, tlsight, ifconfig", Removed),
    ("meta", "spectra", Removed),
    ("base_url", "ifconfig", Removed),
    ("project_name", "ifconfig", Removed),
    ("project_version", "ifconfig", Removed),
    (
        "inspect.request_timeout_secs",
        "spectra",
        Now("[modules.http.inspect].request_timeout_secs"),
    ),
    (
        "inspect.total_timeout_secs",
        "spectra",
        Now("[modules.http.inspect].total_timeout_secs"),
    ),
    (
        "inspect.max_redirects",
        "spectra",
        Now("[modules.http.inspect].max_redirects"),
    ),
    (
        "inspect.user_agent",
        "spectra",
        Now("[modules.http.inspect].user_agent"),
    ),
    (
        "server.admin_bind",
        "ifconfig",
        Now("[server] metrics_bind"),
    ),
    ("server.admin_token", "ifconfig", Removed),
    (
        "circuit_breaker",
        "prism",
        Now("[modules.dns].circuit_breaker"),
    ),
    ("validation", "tlsight", Now("[modules.tls].validation")),
    ("quality", "tlsight", Now("[modules.tls].quality")),
    ("enrichment", "spectra", InProcess),
    ("dnsbl", "beacon", Now("[modules.email].dnsbl")),
    ("http", "beacon", Now("[modules.email].http")),
    ("dkim", "beacon", Now("[modules.email].dkim")),
    ("limits", "prism, tlsight, spectra", Table),
    ("dns", "prism, tlsight, beacon", Table),
    ("trace", "prism", Table),
    ("batch", "ifconfig", Table),
    ("filtered_headers", "ifconfig", Table),
    ("internal_mode", "ifconfig", Table),
    ("watch_data_files", "ifconfig", Table),
    ("dns_base_url", "ifconfig", Table),
    ("tls_base_url", "ifconfig", Table),
    (
        "user_agent_regexes",
        "ifconfig",
        Now("[modules.ip].user_agent_regexes"),
    ),
    (
        "geoip_city_db",
        "ifconfig",
        Now("[modules.ip].geoip_city_db"),
    ),
    ("geoip_asn_db", "ifconfig", Now("[modules.ip].geoip_asn_db")),
    (
        "tor_exit_nodes",
        "ifconfig",
        Now("[modules.ip].tor_exit_nodes"),
    ),
    (
        "cloud_provider_ranges",
        "ifconfig",
        Now("[modules.ip].cloud_provider_ranges"),
    ),
    (
        "feodo_botnet_ips",
        "ifconfig",
        Now("[modules.ip].feodo_botnet_ips"),
    ),
    (
        "cins_army_ips",
        "ifconfig",
        Now("[modules.ip].cins_army_ips"),
    ),
    ("vpn_ranges", "ifconfig", Now("[modules.ip].vpn_ranges")),
    (
        "datacenter_ranges",
        "ifconfig",
        Now("[modules.ip].datacenter_ranges"),
    ),
    ("bot_ranges", "ifconfig", Now("[modules.ip].bot_ranges")),
    (
        "spamhaus_drop",
        "ifconfig",
        Now("[modules.ip].spamhaus_drop"),
    ),
    ("asn_patterns", "ifconfig", Now("[modules.ip].asn_patterns")),
    ("asn_info", "ifconfig", Now("[modules.ip].asn_info")),
];

/// Top-level tables of the retired files that hold key rows: refused when present with no key a
/// row lists, naming the table's fate per origin.
const TABLES: &[(&str, &str)] = &[
    ("scoring", "lens: now `[engine] profile_path`"),
    ("history", "lens: now `[store]`"),
    (
        "cache",
        "lens: now `[admission] cache_enabled` and `cache_window_seconds`; ifconfig: removed in 0.30.0",
    ),
    (
        "rate_limit",
        "lens: now `[admission]`; ifconfig: now `[api.ip]`; beacon: removed in 0.30.0",
    ),
    (
        "backends",
        "now `[engine] resolve_timeout_ms` and `[modules.<m>] deadline_ms`; the `[backends.http]` and `[backends.email]` presence switch is removed in 0.30.0, every registered module runs",
    ),
];

/// The wildcard segment when `pattern` matches `path`, `Some("")` for a pattern without one.
fn matches<'a>(pattern: &str, path: &[&'a str]) -> Option<&'a str> {
    let mut wildcard = "";
    let mut segments = pattern.split('.');
    for part in path {
        let seg = segments.next()?;
        if seg == "<m>" {
            wildcard = part;
        } else if seg != *part {
            return None;
        }
    }
    segments.next().is_none().then_some(wildcard)
}

fn describe(path: &str, found: &[(&str, &Fate, &str)]) -> String {
    let fates: Vec<String> = found
        .iter()
        .map(|(origin, fate, m)| match fate {
            Now(dest) => format!("{origin}: now `{}`", dest.replace("<m>", m)),
            Removed => format!("{origin}: removed in 0.30.0"),
            InProcess => {
                format!("{origin}: removed in 0.30.0, enrichment calls the in-process ip module")
            }
            Table => format!("{origin}: no single destination"),
        })
        .collect();
    format!(
        "legacy configuration key `{path}` ({}); see the CHANGELOG 0.30.0 configuration table",
        fates.join("; ")
    )
}

fn walk(table: &toml::Table, path: &mut Vec<String>) -> Result<(), String> {
    for (key, value) in table {
        path.push(key.clone());
        let segments: Vec<&str> = path.iter().map(String::as_str).collect();
        let found: Vec<(&str, &Fate, &str)> = ROWS
            .iter()
            .filter_map(|(pattern, origin, fate)| {
                matches(pattern, &segments).map(|m| (*origin, fate, m))
            })
            .collect();
        if !found.is_empty() {
            return Err(describe(&path.join("."), &found));
        }
        if let toml::Value::Table(inner) = value {
            walk(inner, path)?;
            if let [name] = path.as_slice()
                && let Some((_, fate)) = TABLES.iter().find(|(table, _)| table == name)
            {
                return Err(format!(
                    "legacy configuration table `[{name}]` ({fate}); see the CHANGELOG 0.30.0 configuration table"
                ));
            }
        }
        path.pop();
    }
    Ok(())
}

/// Fails on the first key path of the raw file that a retired config file used, naming where
/// its value lives now.
pub fn refuse(raw: &toml::Table) -> Result<(), String> {
    walk(raw, &mut Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODULES: [&str; 5] = ["dns", "tls", "http", "email", "ip"];

    /// A valid TOML value for each destination key path (`<m>` substituted).
    const SAMPLES: &[(&str, &str)] = &[
        ("engine.resolve_timeout_ms", "1000"),
        ("engine.profile_path", "\"profile.toml\""),
        ("admission.cache_enabled", "true"),
        ("admission.cache_window_seconds", "300"),
        ("admission.per_ip_per_minute", "10"),
        ("admission.per_ip_burst", "3"),
        ("admission.global_per_minute", "100"),
        ("admission.global_burst", "20"),
        ("api.ip.per_ip_per_minute", "60"),
        ("api.ip.per_ip_burst", "10"),
        ("api.ip.per_target_per_minute", "120"),
        ("api.ip.per_target_burst", "20"),
        ("store.enabled", "true"),
        ("store.db_path", "\"history.db\""),
        ("server.metrics_bind", "\"127.0.0.1:9090\""),
        ("modules.http.inspect.request_timeout_secs", "5"),
        ("modules.http.inspect.total_timeout_secs", "10"),
        ("modules.http.inspect.max_redirects", "5"),
        ("modules.http.inspect.user_agent", "\"netray-test\""),
        ("modules.dns.circuit_breaker", "{ window_secs = 60 }"),
        ("modules.tls.validation", "{ expiry_warning_days = 30 }"),
        ("modules.tls.quality", "{ enabled = true }"),
        ("modules.email.dnsbl", "{ timeout_ms = 2000 }"),
        ("modules.email.http", "{ timeout_ms = 5000 }"),
        ("modules.email.dkim", "{ max_user_selectors = 5 }"),
        ("modules.ip.user_agent_regexes", "\"regexes.yaml\""),
        ("modules.ip.geoip_city_db", "\"city.mmdb\""),
        ("modules.ip.geoip_asn_db", "\"asn.mmdb\""),
        ("modules.ip.tor_exit_nodes", "\"tor.txt\""),
        ("modules.ip.cloud_provider_ranges", "\"cloud.txt\""),
        ("modules.ip.feodo_botnet_ips", "\"feodo.txt\""),
        ("modules.ip.cins_army_ips", "\"cins.txt\""),
        ("modules.ip.vpn_ranges", "\"vpn.txt\""),
        ("modules.ip.datacenter_ranges", "\"dc.txt\""),
        ("modules.ip.bot_ranges", "\"bot.txt\""),
        ("modules.ip.spamhaus_drop", "\"drop.txt\""),
        ("modules.ip.asn_patterns", "\"asn_patterns.toml\""),
        ("modules.ip.asn_info", "\"asn_info.csv\""),
        ("modules.dns.deadline_ms", "2000"),
        ("modules.tls.deadline_ms", "2000"),
        ("modules.http.deadline_ms", "2000"),
        ("modules.email.deadline_ms", "2000"),
        ("modules.ip.deadline_ms", "2000"),
    ];

    /// `[a.b] key`, `[a.b].key` or `[a.b]` as the key path `a.b.key` or `a.b`.
    fn key_path(dest: &str) -> String {
        let (table, rest) = dest
            .strip_prefix('[')
            .and_then(|d| d.split_once(']'))
            .unwrap_or_else(|| panic!("destination {dest:?} does not start with a [table]"));
        let rest = rest.trim().trim_start_matches('.');
        if rest.is_empty() {
            table.to_string()
        } else {
            format!("{table}.{rest}")
        }
    }

    /// Every `Now` destination of `ROWS` as a key path, `<m>` replaced by each module.
    fn destinations() -> Vec<(String, String)> {
        let mut out = Vec::new();
        for (pattern, origin, fate) in ROWS {
            let Now(dest) = fate else { continue };
            let row = format!("{pattern} ({origin}) -> {dest}");
            if dest.contains("<m>") {
                for m in MODULES {
                    out.push((row.clone(), key_path(&dest.replace("<m>", m))));
                }
            } else {
                out.push((row, key_path(dest)));
            }
        }
        out
    }

    /// Keys a table needs beside any other: `[modules.ip]` loads only with both databases.
    const REQUIRED: &[(&str, &str, &str)] = &[
        ("modules.ip", "geoip_city_db", "\"city.mmdb\""),
        ("modules.ip", "geoip_asn_db", "\"asn.mmdb\""),
    ];

    /// The key path `a.b.c` at `value` as a TOML document: `[a.b]`, the keys the table requires,
    /// then `c = value`.
    fn document(path: &str, value: &str) -> String {
        let (table, key) = path.rsplit_once('.').expect("a destination names a table");
        let mut doc = format!("[{table}]\n");
        for (t, k, v) in REQUIRED {
            if *t == table && *k != key {
                doc.push_str(&format!("{k} = {v}\n"));
            }
        }
        doc.push_str(&format!("{key} = {value}\n"));
        doc
    }

    #[test]
    fn key_paths_read_all_three_destination_forms() {
        assert_eq!(
            key_path("[engine] resolve_timeout_ms"),
            "engine.resolve_timeout_ms"
        );
        assert_eq!(
            key_path("[modules.http.inspect].max_redirects"),
            "modules.http.inspect.max_redirects"
        );
        assert_eq!(
            key_path("[modules.dns].circuit_breaker"),
            "modules.dns.circuit_breaker"
        );
    }

    #[test]
    fn every_now_row_has_a_sample_value() {
        let missing: Vec<String> = destinations()
            .into_iter()
            .filter(|(_, path)| !SAMPLES.iter().any(|(p, _)| p == path))
            .map(|(row, path)| format!("{row}: no sample for {path}"))
            .collect();
        assert!(
            missing.is_empty(),
            "add a sample value:\n{}",
            missing.join("\n")
        );
    }

    #[tokio::test]
    async fn every_now_destination_loads_in_check_config() {
        let dir = tempfile::tempdir().unwrap();
        let mut refused = Vec::new();
        for (n, (row, path)) in destinations().into_iter().enumerate() {
            let Some((_, value)) = SAMPLES.iter().find(|(p, _)| *p == path) else {
                continue;
            };
            let file = dir.path().join(format!("dest{n}.toml"));
            std::fs::write(&file, document(&path, value)).unwrap();
            if let Err(e) = crate::serve::check_config(file.to_str().unwrap()).await {
                refused.push(format!("{row}: `{path} = {value}` refused: {e}"));
            }
        }
        assert!(
            refused.is_empty(),
            "stale legacy rows:\n{}",
            refused.join("\n")
        );
    }

    /// The two rows the cutover's branch review caught (specs/solutions/2026-10-22-legacy-row-
    /// names-a-refused-destination.md): their old destinations fail the same check.
    #[tokio::test]
    async fn a_destination_the_config_refuses_is_caught() {
        let dir = tempfile::tempdir().unwrap();
        for (n, dest, value) in [
            (0, "[modules.http].enrichment", "{ enabled = true }"),
            (1, "[api.ip] exempt_cidrs", "[\"192.0.2.0/24\"]"),
        ] {
            let file = dir.path().join(format!("stale{n}.toml"));
            std::fs::write(&file, document(&key_path(dest), value)).unwrap();
            assert!(
                crate::serve::check_config(file.to_str().unwrap())
                    .await
                    .is_err(),
                "{dest} must be refused"
            );
        }
    }
}
