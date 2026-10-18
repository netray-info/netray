# Intent: v2-inspectors

## Problem

V2 shows a run's evidence as one view per protocol and replaces the five V1 inspector SPAs with server-rendered inspectors (R27–R31, S3, S15, S22, S29). Today the evidence exists only as data: `netray-model` holds the six shapes (`crates/model/src/evidence.rs:277`), `netray-web` holds one `evidence_block` macro over its own string view structs (`crates/web/src/components.rs:74`, `templates/components/evidence_block.html`) and nothing maps the one to the other. No route renders an evidence view, no link leads from evidence to a live inspector, and every inspector is still a SolidJS app on its own subdomain (`crates/{dns,tls,http,ip}/frontend`) calling its service's V1 API (`crates/dns/src/api/mod.rs:196`, `crates/tls/src/routes.rs:245`, `crates/http/src/routes.rs:121`, `crates/ip/src/routes.rs:107`).

## Proposed outcome

SDD criterion V4.7: evidence views at `/<domain>/<protocol>` and `/r/<id>/<protocol>` rendered by one renderer over the six shapes (§6.1, R27–R29), explore-live links derived from each block (R30), and the DNS, TLS, HTTP and IP inspectors under `/inspect/<tool>` (§6.2, R31), each job streamed as fragments, never stored, and every inspector run marked "live · not recorded". Built on the templates and islands of the new web layer; verified behaviourally on a local stack with screenshots.

## Affected users and systems

Visitors of evidence views and inspectors once the web layer is routed (v2-cutover). Crates: `netray-web` (renderer, view models, routes, inspector pages, the `dns` island), the module crates' inspector functions (consumed, provided by v2-api), `netray-store` (read only), `tests/acceptance`. The V1 inspector SPAs and their subdomains stay untouched until v2-cutover deletes them.

## Constraints

Built after lanes A (v2-history, v2-admission, v2-entries, v2-api) and B (v2-spectrum, v2-pages) and the operator's UI review at midday 2026-10-18, first in lane C, before v2-result-page (whose Phase 1 needs this feature's Phase 2; operator decisions 2026-10-17); autonomous build towards a local 0.30.0 (operator, 2026-10-17). CSP without `unsafe-eval` and without inline scripts (S25, V4.9); islands compute no layout and hold no verdict (§8.4); the gate of §8.5 holds every template, stylesheet and island. Every job keeps its deadline, its concurrency limit and the fetch policy (P12, P13, P18). Test data names only `example.com` and `netray.info`.

## Open questions

Resolved by the operator on 2026-10-17 (`DECISIONS.md`): v2-api adds findings and evidence references to `RunCheck` (X2); v2-pages owns the `stream` island (X3); the inspector page runs its job through a server-rendered route.
