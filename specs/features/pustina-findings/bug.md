# Bug: pustina-findings

Written by `adlc feature start --bug`. The first four sections are yours; diagnose fills the last three.

## Symptom

A batch of five defects observed on the result page of a DNSSEC-signed domain whose zone has no DS
record in the parent (report 9Mga0Ibv, engine 0.30.0, profile 5):

1. `dns.dnssec` reports `pass` although the `dns.ds` block is empty; `tls.dane_valid` meanwhile says
   "DANE check skipped (requires DNSSEC)", contradicting it.
2. `dns.dnssec_rollover` warns "KSK with key tag 48553 has no matching DS record — new KSK not yet
   published in parent zone" for the zone's only KSK; no rollover is in progress.
3. In dark mode chosen with the theme toggle on a light OS, the `netray` logo in the page header is
   dark glyphs on a dark background, nearly invisible.
4. Every re-check of an unchanged domain shows "RUN 1 · … · FIRST RUN" and the "First run recorded."
   panel, dated with the first recording (2026-10-28), never a history.
5. Lines in "N lines clean · checks that pass" show only status, id, title and protocol · RFC: no
   sentence on what the check verifies, no guide link, no evidence link.
6. The social card `/r/<id>/og.png` is a dark card (`#0d1220`) with a large empty middle where the
   score used to be.

## Expected

1. A signed zone without a DS in the parent makes `dns.dnssec` fail with a finding that the zone is
   signed but not anchored in its parent.
2. `dns.dnssec_rollover` warns about a KSK without DS only while a rollover is plausible (more than one
   KSK, or DS records present); a single KSK with no DS at all leaves it not applicable.
3. The header logo follows the page theme in all four combinations of OS scheme and toggle.
4. A re-check whose result equals the stored run says the domain is unchanged since the first
   recording and when it was last checked, not "first run".
5. Every passing line carries one sentence on what the check verifies, the catalogue's impact
   sentence, a guide link and an evidence link.
6. The social card uses the light palette and fills the former score slot with the protocol grades.

## Reproduction

1. Check a domain signed at its DNS provider but without DS at the registrar; read `dns.dnssec`,
   `dns.dnssec_rollover`, `tls.dane_valid` and the `dns.ds` block.
2. Open any result page on a light OS, choose Theme → dark; look at the header logo.
3. Check a domain, press "Re-check now" without changing the domain; read the run header.
4. Expand "lines clean" on any result page.
5. Open Share → Social card on any result page.

## Environment

Production netray.info, engine 0.30.0, profile 5, Safari on macOS. Example domain: any signed zone
without parent DS (test data uses example.com fixtures). Out of scope here: `email.dnsbl` passing
while Spamhaus refuses the resolver (by design, other zones answered) and the missing IPv6 egress on
the production host (infrastructure).

## Loop

The symptoms show on the running site, so the loop is one shell command against a base URL (default
production) and a domain whose zone is signed without a parent DS (`pustina.de` there). It prints one
`RED <n>` line per symptom of the Symptom list and exits 1 while any remains; `BASE=http://127.0.0.1:<port>`
runs it against a local server. Run from the repository root as a `bash` script:

```sh
B=${BASE:-https://netray.info}; D=${DOMAIN:-pustina.de}; red=0
e=$(curl -fsS "$B/api/v1/domains/$D"); p=$(curl -fsS "$B/$D")
st() { jq -r --arg i "$1" '[.. | objects | select(.id==$i and has("status"))][0].status' <<<"$e"; }
ds=$(jq '[.. | objects | select(.id=="dns.ds" and has("shape"))][0].shape.data | length' <<<"$e")
id=$(jq -r .id <<<"$e")
r() { echo "RED $1"; red=1; }
[ "$ds" = 0 ] && [ "$(st dns.dnssec)" = pass ] && r "1 dns.dnssec pass with empty dns.ds"
[ "$ds" = 0 ] && [ "$(st dns.dnssec_rollover)" != not_applicable ] && r "2 rollover reads a missing DS as a rollover"
grep -q '<img src="/static/logo.svg"' <<<"$p" && r "3 header logo is an <img>, blind to data-theme"
[ "$(jq '.last_seen_at > .created_at' <<<"$e")" = true ] && grep -q 'First run recorded' <<<"$p" && r "4 unchanged re-check shown as first run"
[ "$(sed -n '/class="clean-bar/,/<\/details>/p' <<<"$p" | grep -c '/guide/')" = 0 ] && r "5 pass lines carry no guide link"
curl -fsS "$B/r/$id/og.png" | python3 -I -c '
import sys,zlib,struct
d=sys.stdin.buffer.read();i=8;z=b""
while i<len(d):
  n,t=struct.unpack(">I4s",d[i:i+8]);z+=d[i+8:i+8+n] if t==b"IDAT" else b"";i+=12+n
p=zlib.decompress(z)[1:4];sys.exit(0 if sum(p)/3<128 else 1)' && r "6 social card background is dark"
exit $red
```

Run twice on 2026-11-01 against https://netray.info (engine 0.30.0), identical both times:

```
RED 1 dns.dnssec pass with empty dns.ds
RED 2 rollover reads a missing DS as a rollover
RED 3 header logo is an <img>, blind to data-theme
RED 4 unchanged re-check shown as first run
RED 5 pass lines carry no guide link
RED 6 social card background is dark
exit 1
```

## Hypotheses

The batch carries six symptoms; one hypothesis per symptom, plus the two neighbours that would have
explained them differently. Each was decided by the read named with it.

| # | Hypothesis | Refuted if | Decided by | Verdict |
|---|---|---|---|---|
| H1 | `dns.dnssec` comes from mhost's `check_dnssec` lint, whose DS binding check returns nothing for an empty DS set | the lint fails on empty DS, or dns.dnssec is fed by `dns_dnssec.rs` | `sed -n 1204p crates/dns/src/api/check.rs`; in mhost 0.12.1 (`Cargo.lock`) `validate_ds_dnskey_binding` opens with `if ds_records.is_empty() { return Vec::new(); }` | held |
| H1b | the chain validator `crates/dns/src/dns_dnssec.rs` (which fails on "No DS records found in parent zone") judges dns.dnssec | `lint_lookups` does not call it | `sed -n 1197,1215p crates/dns/src/api/check.rs`: the `dnssec` category is mhost's `check_dnssec(lookups)` | refuted |
| H2 | `check_dnssec_rollover` warns for every KSK whose tag is not among the DS tags, regardless of KSK count or an empty DS set | the warning is guarded by KSK count or DS presence | `sed -n 1664,1670p crates/dns/src/api/check.rs` | held |
| H3 | the header embeds `logo.svg` through `<img>`, so the SVG's own `prefers-color-scheme` rule cannot see the page's `data-theme` | the logo is inline or switched by page CSS | `sed -n 18p crates/web/templates/base.html`; loop symptom 3 on the served page | held |
| H4 | an unchanged re-check records `Identical` (no new entry) and the page sets `first` from the entry count alone | prod shows several entries, or `first` reads `recorded` when the count is known | `curl -s https://netray.info/api/v1/domains/pustina.de`: one entry, `created_at` 1793215898 < `last_seen_at` 1793523321, `predecessor_id` null; `sed -n 1001,1006p crates/web/src/pages/result/mod.rs` | held |
| H4b | the store is lost on each redeploy, so every run is a first run | the entry predates the last deploy and survives re-checks | same curl: the entry of 2026-10-28 is still served on 2026-11-01 with a later `last_seen_at` | refuted |
| H5 | `passed_checks` builds pass rows with `fix`, `evidence_href` and `guide_href` set to `None` and a body from findings only, and the catalogue has no positive "what it checks" text | a pass row gets a guide link or catalogue text | `sed -n 268,290p crates/web/src/pages/result/views.rs`; `CheckSpec` fields in `crates/engine/src/catalogue.rs` | held |
| H6 | the card background is the V1 constant `#0d1220` and the strip uses the dark tokens | the card reads a light token | `sed -n 40,42p crates/web/src/og/layout.rs`; `crates/web/src/plate.rs` imports `COLOR_DARK_PLATE`/`STATUS_DARK_*` | held |

## Root cause

Six independent causes, one per symptom.

1. `dns.dnssec` is the mhost 0.12.1 lint `check_dnssec`, whose DS/DNSKEY binding check returns no
   finding when the parent publishes no DS; an unanchored signed zone therefore passes. Reached here:

```quote crates/dns/src/api/check.rs:1204
        ("dnssec", check_dnssec(lookups)),
```

2. The rollover check reads every KSK without a DS as a newly published key:

```quote crates/dns/src/api/check.rs:1666
    for &ksk_tag in &ksk_tags {
        if !ds_tags.contains(&ksk_tag) {
            results.push(CheckResult::Warning(format!(
                "KSK with key tag {ksk_tag} has no matching DS record — new KSK not yet published in parent zone"
```

3. The logo is an external image; the page's theme toggle cannot reach into it:

```quote crates/web/templates/base.html:18
<a href="{{ frame.origin() }}/"><img src="/static/logo.svg" alt="netray"></a>
```

4. An unchanged re-check adds no entry (intended, SDD v2 R10), and the page derives "first run" from
   the entry count alone, discarding the `Identical` outcome:

```quote crates/web/src/pages/result/mod.rs:1002
    let ordinal = ordinal_known.then_some(entries.len());
    let first = match ordinal {
        Some(n) => n == 1,
        None => matches!(recorded, Some(Recorded::First { .. })),
    };
```

5. Pass rows are built without explanation, guide or evidence:

```quote crates/web/src/pages/result/views.rs:285
                fix: None,
                meta: spec_at.map(|i| meta_text(&catalogue.entries()[i])),
                evidence_href: None,
                guide_href: None,
```

6. The social card keeps the V1 dark palette:

```quote crates/web/src/og/layout.rs:41
pub const BG_COLOR: &str = "#0d1220";
```
