### Reader

COUNTS blockers=3 amendments=0 deferred=0 nits=4
LENSES Engineering

BLOCKER | crates/engine/catalogue/checks.toml:405 | The new `checks` sentence for `tls.caa_compliant` tells the reader the issuing CA "is authorised by the domain's CAA records". The check passes exactly when no CAA record exists at all (`caa_compliance.rs:39`, RFC 8659 absence is permissive), so the pass row states something false on the path R6 is for. | Take a domain with no CAA record at any level of the climb and a valid certificate: `tls.caa_compliant` passes, and its clean-list row reads "The issuing certificate authority is authorised by the domain's CAA records." On the same page `dns.caa` warns "The domain publishes no CAA records".
BLOCKER | crates/engine/catalogue/checks.toml:575 | The new `checks` sentence for `http.referrer_policy` claims the effective policy "does not leak full URLs to other origins". The check counts `no-referrer-when-downgrade` as a pass value (`security.rs:36`), and that policy sends the full URL (path and query) to every HTTPS cross-origin target. | Send `Referrer-Policy: no-referrer-when-downgrade`: `http.referrer_policy` passes, and its pass row says the policy does not leak full URLs to other origins.
BLOCKER | crates/engine/catalogue/checks.toml:661 | The new `checks` sentence for `email.mx` claims the MX records are "spread over more than one server". The check passes a single MX host that has more than one address: `single_mx` is only Info (`mx.rs:188`), `low_network_diversity` counts one v4 prefix plus one v6 prefix as 2 and stays silent, and `fold_category` makes `mx` Pass while keeping the Info finding. | Publish one MX `mail.example.com` with one A and one AAAA record, i.e. a single dual-stack server: `email.mx` passes, and the row reads "...spread over more than one server. If it failed: ... only one MX record, but it resolves to 2 IP addresses".
NIT | crates/engine/catalogue/checks.toml:53 | The new `checks` sentence for `dns.caa` claims the CAA records "restrict which certificate authorities may issue". mhost 0.12.1 `check_caa` passes any non-empty CAA set that has no unknown tag and does not have `issue` without `issuewild`, so a set without any `issue` property passes even though it restricts no CA. | A zone whose only CAA record is `0 iodef "mailto:security@example.com"` gets `dns.caa` pass (mhost: "Found 1 CAA record(s) restricting certificate issuance"), and its row claims the records restrict which CAs may issue.
NIT | crates/engine/catalogue/checks.toml:505 | The new `checks` sentence for `http.cors` reads most naturally as "does not reflect untrusted origins". The code passes a response that echoes the probe Origin without `Access-Control-Allow-Credentials` (`cors.rs:46`, "CORS configured"). | A probe answered with `Access-Control-Allow-Origin: https://evil.example.com` and no ACAC gets `http.cors` pass, and its row says the site does not reflect untrusted origins.
NIT | crates/web/src/plate.rs:44 | The card's protocol grades skip the guard the page applies to the same grades. `section_grades` (`mod.rs:836`) shows none when the profile is unknown or when `score_run`'s grade differs from the stored letter. `card_layout` falls back to the current profile and prints the grades anyway, next to the stored letter. | Take a stored entry whose stored grade is B and which `score_run` re-scores to A after a later engine change. The result page hides the section grades. `/r/<id>/og.png` draws a large B beside "DNS A+ · TLS A+ · …", grades the page withholds as contradicting the letter.
NIT | specs/sdd/v2.md:690 | The amended SDD says the card shows the grades of "the protocols present in the run". The code draws only scored sections (`score_run(...).sections`), which leave out sections that are not applicable, incomplete, or have a weighted check unmeasured. | In a run whose HTTP section is measured but has `http.https_redirect` unmeasured (port-80 probe timeout), the card reads "DNS A+ · TLS A · Email B · IP A+" without HTTP, although HTTP is present in the run.

```quote crates/engine/catalogue/checks.toml:405
checks = "The issuing certificate authority is authorised by the domain's CAA records."
```

```quote crates/tls/src/validate/caa_compliance.rs:39
    if caa.is_empty() {
        return CheckStatus::Pass;
    }
```

```quote crates/engine/catalogue/checks.toml:575
checks = "Responses carry a Referrer-Policy header whose effective policy does not leak full URLs to other origins."
```

```quote crates/http/src/inspect/security.rs:36
            let pass_values = [
                "no-referrer",
                "no-referrer-when-downgrade",
```

```quote crates/engine/catalogue/checks.toml:661
checks = "The domain's MX records resolve to mail hosts with addresses, spread over more than one server."
```

```quote crates/email/src/checks/mx.rs:188
    if mx_records.len() == 1 && addrs_measured {
        let (verdict, detail) = if all_ips.len() > 1 {
            (
                Verdict::Info,
                format!(
                    "only one MX record, but it resolves to {} IP addresses",
                    all_ips.len()
                ),
```

```quote crates/engine/catalogue/checks.toml:53
checks = "The domain publishes CAA records that restrict which certificate authorities may issue for it."
```

```quote crates/engine/catalogue/checks.toml:505
checks = "The site's CORS policy does not reflect untrusted origins or allow arbitrary origins with credentials."
```

```quote crates/http/src/inspect/cors.rs:46
        (CheckStatus::Pass, "CORS configured".to_string())
```

```quote crates/web/src/plate.rs:44
    let profile = Profiles::get(run.profile_version).unwrap_or_else(Profiles::current);
```

```quote crates/web/src/pages/result/mod.rs:836
    if shown.is_some_and(|shown| score.grade != shown) {
```

```quote specs/sdd/v2.md:690
protocol grades (`DNS A+ · TLS A+ · …`, the protocols present in the run), and the card draws above them the plate of `spectrum::layout` (V4.4) for the entry's run over its
```
