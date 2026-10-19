# Plan: v2-http-checks

## Phase 1 — Check fixes: Referrer-Policy lists, inspection duration, Set-Cookie rows

### Groups

G1: C4, C17, C18 · G2: C1, C6-C10 · G3: C2, C3, C11-C16 · G4: C5 (SDD, prose)

### Plan

- G1: `crates/model/src/canonical.rs` `table`: a volatile row path also drops rows keyed `<row>#<digits>` (helper `row_covers`); fingerprint and diff both read the canonical form.
- G2: `crates/http/src/inspect/security.rs` Referrer-Policy scorer: last known comma-separated token decides; "Permissive policy: <token>"; "Unrecognized value: <value>". Feeds V1 quality check and `http.referrer_policy`.
- G3: `crates/http/src/module.rs` `REGISTER`, `VOLATILE`, `server_block` (`inspection_ms`), `headers_block` (`set-cookie`, `set-cookie#<n>` rows); `crates/lens/src/modules.rs` `HttpServer` reads `inspection_ms`; `crates/http/tests/evidence.rs` volatile list (test change); lens full goldens regenerate with `UPDATE_GOLDEN=1 cargo test -p lens --test lens_full_output`.
- G4: `specs/sdd/v2.md` §3.7 (done with the code commit).

## Phase 2 — Parsed evidence: CSP, HSTS, caching, CDN, isolation headers

### Groups

G1: C1, C10, C11, C12 (`csp.rs`) · G2: C2-C8, C13-C25 (`module.rs`, depends on G1) · G3: C9 (SDD, prose, with the code commit)

### Plan

- G1: `crates/http/src/inspect/csp.rs`: `pub enum CspSeverity {Fail, Warn}`, `pub struct CspIssue {directive, severity, message}`, `pub fn issues(&IndexMap<String, Vec<String>>) -> Vec<CspIssue>` replacing the prefix-string `score_directives`; `analyze_csp` takes status from severities and report-only, `issues` strings = messages in order (V1 texts unchanged).
- G2: `crates/http/src/module.rs`: `REGISTER` blocks `http.csp` (Tree), `http.hsts`, `http.caching`, `http.cdn`, `http.isolation` (Fields) with the spec's keys; `VOLATILE` adds `http.caching/etag`, `/last_modified`, `/age`, `http.cdn/cache_status`, `http.csp/source`; `blocks` emits the five only when answered (`csp_block` from `resp.security.csp` and `csp::issues`, `hsts_block` when `resp.headers` holds `strict-transport-security`, `caching_block` from `resp.caching`, `cdn_block` from `resp.cdn`, `isolation_block` from `security.coop/coep/corp`); `references` adds `http.hsts`, `http.csp`, `http.caching` beside `http.headers` (absent blocks dropped by the existing retain); `check_result` for `csp` with a policy and no probe override: findings = report-only finding + issue messages.
- G3: `specs/sdd/v2.md` §3.7 (drafted).
