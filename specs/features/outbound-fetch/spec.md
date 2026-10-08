# Spec: one outbound fetch policy

Status: Ready for Implementation
Created: 2026-10-08

Four places fetch a URL that comes from a checked domain's records or certificates: beacon's MTA-STS policy fetch, beacon's BIMI logo fetch, spectra's redirect hops and tlsight's live OCSP request. Each builds its own client and decides on its own which targets it accepts. This spec gives them one fetch helper in `netray-common` with one target policy, applied to every hop, and with the connection pinned to the checked address.

A fetch the policy allows keeps today's result. A fetch the policy refuses gets the checker's refused result (requirement 8).

## Requirements

1. `netray_common::fetch` (feature-gated) fetches a URL for a caller. It parses with `url::Url`, resolves every host through a checking resolver that refuses an empty set or any address failing `target_policy` and otherwise hands reqwest exactly the checked addresses, checks IP-literal hosts itself, and builds the client from typed settings so no caller can route around the check. A refused target returns a typed `Blocked` error, and no connection is made to it.
2. reqwest follows redirects with a custom policy, so Location handling, method rules, header stripping and Referer stay as today. Every hop's host is checked again: names through the checking resolver, literals in the policy. The caller sets the hop limit and what reaching it does: `Fail`, or `ReturnLast` (the last 3xx response is returned with a limit-reached flag). The caller also sets HTTPS-only and a body cap.
3. beacon's MTA-STS policy fetch goes through the helper: limit 0 with `ReturnLast` (a 3xx keeps today's `https_redirect` Fail), HTTPS only, 64 KB cap. A refused or unresolvable policy host gives `https_fetch_failed` Fail with the detail "policy host not reachable". The existing `ssrf_blocked` sub-check keeps its name and verdict.
4. beacon's BIMI logo fetch goes through the helper: HTTPS only, at most 4 redirects (today's limit). The custom redirect client and the string-based host extraction are removed. A refused initial host that resolves to a non-public address keeps `logo_ssrf_blocked` Fail. A redirect hop refused for a non-public address (resolved or literal) gives `logo_redirect_ssrf_blocked` Fail; a redirect hop with no address gives `logo_unreachable` Warn, as before. Any other refused or unresolvable target gives `logo_unreachable` Warn with the detail "logo host not reachable". No response status or connection error text from a refused target appears in a detail.
5. spectra's redirect following goes through the helper with `ReturnLast` at its `max_redirects`. It keeps its hop recording (`RedirectHop`). A refused hop ends the chain with the existing "Redirect destination blocked" result.
6. tlsight's live OCSP request goes through the helper: HTTP allowed, up to 10 redirects (today's limit) with today's redirect semantics (301/302/303 continue as GET without body, 307/308 keep the POST), 64 KB cap. A refused URL or hop gives status `unknown` with reason `blocked`.
7. A results table test drives the real `check_bimi`, `check_mta_sts` and spectra's redirect follower against a stub resolver and local listeners, and records per row the sub-check name, verdict and beacon grade (beacon), or the final status, limit flag, hop list and check verdicts (spectra). It runs against today's check code: the only production change it needs is a behaviour-neutral extraction of beacon's two client builders (`state::http_client_builder`, `state::http_client_follow_builder`), so a test can add a local resolve map and a test certificate to the production builder.
9. prism's MTA-STS policy fetch goes through the helper: GET, up to 10 redirects with `Fail` (reqwest's default today), 5 s total, 64 KB cap. A refused target gives today's unreachable result, "MTA-STS policy file unreachable" (Warning), without any error text; a timeout keeps "MTA-STS policy file fetch timed out". No `reqwest::Client` remains in prism.
8. Refusal rule: every fetch the policy refuses, initial target or any redirect hop, whatever the host form (a name resolving to any non-public address, an empty resolution, an IP literal, userinfo, a bracketed IPv6 literal), gets the checker's refused result from requirements 3–6 and 9; every fetch the policy allows keeps today's result, with these stated exceptions (operator, 2026-10-09): spectra sends every redirect hop to its own URL's port, where the old client carried the first hop's port to every same-host hop (TLS to :80, plaintext to :443); failure details no longer carry reqwest's error text; prism's policy fetch now has one 5 s deadline and a 64 KiB cap over the whole body. After this spec the results table is unchanged except rows whose target the policy refuses.

## Phase 1 — Results table

**Depends on:** none
**Requirements:** 7

### Test Scenarios

- GIVEN beacon with a stub resolver WHEN the BIMI logo host does not answer THEN the table row is `logo_unreachable` Warn.
- GIVEN beacon WHEN the BIMI logo host is NXDOMAIN THEN the row records today's sub-check, verdict and grade.
- GIVEN beacon WHEN the logo host resolves to `10.0.0.1` THEN the row is `logo_ssrf_blocked` Fail.
- GIVEN beacon WHEN the final redirect target resolves to `10.0.0.1` and answers 200, or answers 404 THEN each row records today's result.
- GIVEN beacon WHEN an intermediate hop resolves to `10.0.0.1` and the chain ends in a public 200 THEN the row records today's result.
- GIVEN beacon WHEN the logo is behind 4 redirects, and behind 5 THEN each row records today's result.
- GIVEN beacon WHEN the logo URL uses `127.0.0.1`, `x@127.0.0.1` or `[::1]` THEN each row records today's result.
- GIVEN beacon WHEN the beacon resolver errors on a public logo host THEN the row records today's result.
- GIVEN beacon WHEN a redirect goes to the IP literal `https://10.1.2.3/l.svg` answering 200 THEN the row records today's result.
- GIVEN beacon WHEN the MTA-STS policy host resolves to `[]` with a system-resolvable public host, to `10.0.0.1`, or to a public address, or the policy endpoint answers 301 THEN each row records today's result.
- GIVEN spectra WHEN a chain of 3 redirects ends in 200, a redirect loop meets `max_redirects` 10, a redirect goes to `http://localhost:<p2>/`, or a redirect goes to a name resolving to `[public, 10.0.0.1]` THEN each row records today's final status, limit flag, hop list and verdicts.

## Phase 2 — Fetch helper

**Depends on:** Phase 1
**Requirements:** 1, 2

### Test Scenarios

Every refused case asserts the typed `Blocked` error and that a dual-stack listener on `127.0.0.1` and `[::1]` sees 0 connections.

- GIVEN `https://127.0.0.1:<p>/` WHEN fetched THEN `Blocked`, 0 connections.
- GIVEN `https://x@127.0.0.1:<p>/` WHEN fetched THEN `Blocked`, 0 connections.
- GIVEN `https://[::1]:<p>/` WHEN fetched THEN `Blocked`, 0 connections.
- GIVEN a name the stub resolver maps to `[]` WHEN fetched THEN `Blocked`.
- GIVEN a name mapped to `[public, 127.0.0.1]` WHEN fetched THEN `Blocked`.
- GIVEN `pinned.invalid` stub-mapped to an allowed listener WHEN fetched in test mode THEN it succeeds (`.invalid` cannot resolve through the system, so success proves the pin).
- GIVEN an allowed hop redirecting to `localhost:<p>` or `http://svc.invalid/` (stub → loopback) WHEN fetched THEN `Blocked`, 0 connections at the target.
- GIVEN limit 4 and `Fail` WHEN 4 redirects end in 200 THEN success; WHEN 5 THEN error.
- GIVEN limit 2 and `ReturnLast` WHEN 3 redirects THEN the third 3xx, the one not followed, is returned with the limit flag set and two hops, as spectra counts today.

## Phase 3 — Callers

**Depends on:** Phase 2
**Requirements:** 3, 4, 5, 6, 8, 9

### Test Scenarios

- GIVEN the MTA-STS policy host maps to `[]` WHEN checked THEN `https_fetch_failed` Fail, detail "policy host not reachable", 0 connections.
- GIVEN the MTA-STS policy host maps to a public address WHEN checked THEN the request goes to that pinned address.
- GIVEN a BIMI `l=` of `https://127.0.0.1:<p>/l.svg`, `https://x@127.0.0.1:<p>/`, `https://[::1]:<p>/` or `https://internal.invalid/` (stub → `[]`) WHEN checked THEN the results-table verdict, no internal status in the detail, 0 connections.
- GIVEN a public BIMI logo URL that redirects to loopback WHEN checked THEN `logo_redirect_ssrf_blocked` Fail before the second hop connects.
- GIVEN a spectra target answering `302` to `http://localhost:<p2>/`, `http://[::1]:<p2>/` or `http://svc.invalid/` WHEN inspected THEN "Redirect destination blocked", 0 connections at `p2`.
- GIVEN an allowed spectra chain `A → 301 → B → 302 → C → 200` WHEN inspected THEN it ends at `C` and `redirects` is exactly `[A→B (301), B→C (302)]` with URL, status and location.
- GIVEN an OCSP URL `http://127.0.0.1:<p>/` or `http://ocsp.invalid/` (stub → loopback) WHEN checked THEN `unknown`/`blocked`, 0 connections.
- GIVEN an OCSP responder answering `301` to an allowed second responder WHEN checked THEN the second responder receives a GET without body; with `307` it receives the POST.
- GIVEN an MTA-STS policy endpoint answering 301 WHEN checked THEN `https_redirect` Fail, as today.
- GIVEN a spectra hop to a name resolving to `[public, 10.0.0.1]` WHEN inspected THEN "Redirect destination blocked", 0 connections.
- GIVEN the Phase 1 results table WHEN run THEN it is green, and only rows with a refused target changed, each to its refused result (`ADLC-Test-Change` naming requirement 8).

- GIVEN prism checks a domain whose `mta-sts.` host resolves to `10.0.0.1`, or whose public policy host redirects to loopback WHEN `+check` runs THEN the MTA-STS result is "MTA-STS policy file unreachable" (Warning), with no status or error text, and the target sees 0 connections.
- GIVEN spectra follows `http://a/` → 301 → `https://b/` and `b` refuses the connection WHEN inspected THEN `redirects` still holds `[a→b 301]` and `redirects_to_https` is true, as today.
- GIVEN spectra's port-80 probe is answered with 301 to `https://10.0.0.1/` WHEN inspected THEN the refused hop is not recorded, `redirects_to_https` is false, as today.

## Open decisions

None.
