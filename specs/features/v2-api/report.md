# Report: v2-api

## Phase 1 — API module, stored findings, contract harness, catalogue, profiles, meta

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: API v1 in netray-web `api::router(ApiState)`, `DomainRuns` seam, lens `validate`, netray builds parts, lens names no module | green | crates/web/tests/api_domain_runs.rs, crates/lens/tests/api_v1_runs_validate.rs |
| C2 | R2: OpenAPI 3.1 by utoipa, title "netray API", workspace version, `/api/v1` paths, served + docs, ErrorResponse `$ref`, no untyped schema | green | crates/web/tests/openapi_document.rs |
| C3 | R3: model and engine types derive ToSchema; CheckSpec/Profile Serialize; goldens byte-identical | green | crates/engine/tests/schema_derives.rs |
| C4 | R4: `RunCheck.findings`/`.evidence`, optional, copied by build_run, ignored by fingerprint and diff | green | crates/lens/tests/history_findings.rs, crates/model/tests/run_check_fields.rs |
| C5 | R5: committed `api-v1.openapi.json`, compatibility check + equality check, UPDATE_GOLDEN only when compatible | green | crates/web/tests/openapi_contract.rs, crates/web/tests/contract_edges.rs |
| C6 | R6: every endpoint's responses validated against the fixture (jsonschema); coverage guard | green | crates/web/tests/openapi_responses.rs |
| C7 | R7: every non-2xx v1 answer carries the error shape; unknown `/api/v1/` path is 404 JSON, outside reaches SPA | green | crates/lens/tests/api_v1_mount.rs, crates/web/tests/api_edges.rs, crates/lens/tests/api_v1_edges.rs |
| C8 | R8: `[api]` config (enabled, per-IP limiter, inspector tables), deny_unknown_fields, enabled=false serves no v1 JSON | green | crates/lens/tests/api_v1_limits.rs, crates/lens/tests/api_config.rs, crates/lens/tests/api_v1_disabled.rs |
| C9 | R9: per-client limits keyed by `client_ip_from`'s address | green | crates/lens/tests/api_v1_limits.rs |
| C10 | R15: `test_api_no_verdict.sh` with self-test fixtures | green | tests/repo/test_api_no_verdict.sh |
| C11 | R16: `GET /catalogue` every CheckSpec in file order, eleven fields | green | crates/web/tests/api_reads.rs |
| C12 | R17: `GET /profiles/{n}`; unknown 404; non-integer 400 INVALID_INPUT | green | crates/web/tests/api_reads.rs, crates/web/tests/api_edges.rs |
| C13 | R18: `GET /meta` fields and limits | green | crates/web/tests/api_reads.rs, crates/lens/tests/api_v1_meta.rs |
| C14 | R32: every V1 API unchanged; lens and contract goldens byte-identical | green | crates/lens/tests/api_v1_mount.rs (+ lens_golden.rs, contract_*.rs) |
| C15 | R33: SDD §3.1, §3.5/§4.1 and §10 amended | green | tests/repo/test_api_sdd.sh |
| C16 | R35: `/api/v1/docs` Scalar page, relaxed CSP, unlimited, not a document path, absent when disabled | green | crates/lens/tests/api_v1_docs.rs |
| C17 | S: `/api/v1/openapi.json` equals the committed fixture ignoring info.version | green | crates/web/tests/openapi_contract.rs |
| C18 | S: deleted response property fails naming path and property; UPDATE_GOLDEN leaves fixture unchanged | green | crates/web/tests/openapi_contract.rs |
| C19 | S: property type integer→string fails | green | crates/web/tests/openapi_contract.rs |
| C20 | S: new optional response property passes compat, fails equality naming the command, passes after UPDATE_GOLDEN | green | crates/web/tests/openapi_contract.rs |
| C21 | S: request property becoming required fails | green | crates/web/tests/openapi_contract.rs |
| C22 | S: untyped-schema walk fails on `{"type":"object"}` naming location; passes on the repository's document | green | crates/web/tests/openapi_document.rs, crates/web/tests/contract_edges.rs |
| C23 | S: coverage test fails naming a path without 2xx and error case | green | crates/web/tests/openapi_responses.rs |
| C24 | S: `/api/v1/docs` 200 text/html loading the document, relaxed CSP vs strict on catalogue, 11 burst unlimited, no `/docs` path; disabled not served | green | crates/lens/tests/api_v1_docs.rs |
| C25 | S: `/catalogue` lists every check of checks.toml in file order with eleven fields, validates | green | crates/web/tests/api_reads.rs |
| C26 | S: `/profiles/3` weights equal v3.toml; `/profiles/99` 404; `/profiles/x` 400 | green | crates/web/tests/api_reads.rs |
| C27 | S: `/meta` api v1, workspace version, profiles contains 3, limits 30 and 8 under default config | green | crates/web/tests/api_reads.rs, crates/lens/tests/api_v1_meta.rs |
| C28 | S: `/api/v1/nope` 404 JSON; `/nope` SPA index | green | crates/lens/tests/api_v1_mount.rs, crates/lens/tests/api_v1_edges.rs |
| C29 | S: `[api] enabled=false`: no v1 JSON; `/api/check/example.com` as before | green | crates/lens/tests/api_v1_mount.rs, crates/lens/tests/api_v1_disabled.rs |
| C30 | S: 11th `/catalogue` in a burst 429 RATE_LIMITED + Retry-After; 11 `/openapi.json` none limited | green | crates/lens/tests/api_v1_limits.rs |
| C31 | S: X-Real-Ip from trusted proxy with spoofed CF-Connecting-IP keys on X-Real-Ip | green | crates/lens/tests/api_v1_limits.rs |
| C32 | S: unknown key under `[api]` or `[api.ip]` rejected; `--check-config` exits 1 naming the key | green | crates/lens/tests/api_config.rs, tests/repo/test_check_config.sh |
| C33 | S: Run JSON without findings/evidence parses empty and reserialises to the same bytes | green | crates/model/tests/run_check_fields.rs |
| C34 | S: recorded stub run stores warn message in findings and `dns.caa` in evidence; N/A section's check carries neither | green | crates/lens/tests/history_findings.rs |
| C35 | S: runs differing only in findings/evidence: equal fingerprints, empty diff, Identical; status change differs | green | crates/store/tests/findings_identical.rs |
| C36 | S: no-verdict check flags planted fixture, passes clean, ignores outside src/api, passes repository | green | tests/repo/test_api_no_verdict.sh |
| C37 | S: lens `/api-docs/openapi.json` lists no `/api/v1` path; lens and contract goldens unchanged | green | crates/lens/tests/api_v1_mount.rs |
| C38 | S: test_web_crate.sh passes with axum, Router under api/, netray→netray-web; fails on planted Router in components.rs or edition≠2024; lens-no-module unchanged | green | tests/repo/test_web_crate.sh |

### Behavioural verification

`netray lens` built from the branch, started on `crates/lens/lens.example.toml` with the binds moved to 127.0.0.1:18185/19195:

```
$ netray lens --check-config lens-bv.toml   -> config ok (rc 0)
$ curl /api/v1/meta
{"version":"0.26.0","api":"v1","current_profile":3,"profiles":[3],"evidence_schema_version":1,"limits":{"api":{"per_ip_per_minute":60,"per_ip_burst":10},"admission":{"client_budget_per_hour":30,"domain_run_slots":8},"rate_limit":{"per_ip_per_minute":10,"per_ip_burst":3,"global_per_minute":100,"global_burst":20}}}
$ curl -i /api/v1/profiles/x  -> 400 application/json {"error":{"code":"INVALID_INPUT","message":"profile version `x` is not an integer"}}
$ curl -i /api/v1/nope        -> 404 application/json {"error":{"code":"NOT_FOUND","message":"no such endpoint"}}
$ curl -D- /api/v1/docs       -> 200, CSP script-src 'self' 'unsafe-inline' https://cdn.jsdelivr.net
$ 11x /api/v1/catalogue       -> 200 x8, 429 x3 (two limited requests before: meta, profiles/x; burst 10)
```

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 model/engine derives, RunCheck fields | 3 | opus (inherited) | 93409 | 260 |
| G2 netray-web api module, contract harness | 2 | opus (inherited) | 115482 | 269 |
| G3 lens wiring, netray, SDD | 2 | opus (inherited) | 94602 | 977 |
| review fixes (pass 1) | 1 | opus (inherited) | 76786 | 178 |
| review fix (pass 2): v1 off leaves 413 and CSP alone | 1 | orchestrator | - | - |

### Review

Pass 1 (reader over the phase diff): 6 BLOCKER, 7 AMENDMENT, 1 DEFERRED, 2 NIT. Blockers fixed through a new failing-test commit (`/api/v1/` trailing slash, 405 error shape, invalid UTF-8 path, contract parameters, one-sided `format`/`items`, annotation-only untyped schemas). Amendments repaired in this phase and logged in the spec's Decision log (413 code, no-verdict patterns, configured scoring profile, digits-only profile versions, `oneOf` by content, `lens_rate_limit_hits_total{scope="api_v1"}`); stale findings on `Identical` kept per X2 and logged.
Pass 2: 1 BLOCKER (with `[api] enabled = false` the 413 rewrite still answered JSON under `/api/v1`), fixed test-first; 2 AMENDMENT (413 documented on every v1 operation; contract errors name the component) repaired and logged; 3 NIT listed.
Deferred: `ApiParts` empty and the `[api.<inspector>]` tables keyless until Phases 4-7; `/meta` limits gain the IP and inspector values then.
NITs: `untyped_schemas` does not walk `propertyNames` or response `headers`; the no-verdict grep is line-based (a rustfmt-wrapped grouped import escapes it); `Retry-After` rounds governor's wait down (inherited helper).
