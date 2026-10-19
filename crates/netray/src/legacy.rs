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
    (
        "rate_limit.exempt_cidrs",
        "ifconfig",
        Now("[api.ip] exempt_cidrs"),
    ),
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
    ("enrichment", "spectra", Now("[modules.http].enrichment")),
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
