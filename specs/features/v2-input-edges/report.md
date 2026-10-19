# Report: v2-input-edges

## Phase 1 — IDNA mapping in the domain rule

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 IDNA mapping: validate maps the trimmed input with UTS #46 after the raw address checks and returns the mapped ASCII form; a mapping error refuses with not a valid domain name | green | crates/common/tests/it/domain_idna.rs |
| C2 | R2 Mapped addresses: the address and numeric-last-label checks run again on the mapped form | green | crates/common/tests/it/domain_idna.rs |
| C3 | R3 Whitespace and forbidden characters inside are refused | green | crates/common/tests/it/domain_idna.rs |
| C4 | R4 One domain, one form: full-width, ideographic full stop, uppercase and Unicode labels map to one ASCII form | green | crates/common/tests/it/domain_idna.rs |
| C5 | R5 Every caller: lens API v1 runs, result page and evidence page see the mapped form | green | crates/lens/tests/it/idna_domain_routes.rs |
| C6 | GIVEN １.１.１.１ or １.１.１.１. WHEN validated THEN refused as an IP address | green | crates/common/tests/it/domain_idna.rs |
| C7 | GIVEN ０x７f.1 WHEN validated THEN refused as a numeric last label | green | crates/common/tests/it/domain_idna.rs |
| C8 | GIVEN whitespace, slash, percent or tab inside WHEN validated THEN refused with not a valid domain name | green | crates/common/tests/it/domain_idna.rs |
| C9 | GIVEN ＥＸＡＭＰＬＥ.com, example。com or Example.COM. WHEN validated THEN example.com | green | crates/common/tests/it/domain_idna.rs |
| C10 | GIVEN bücher.de or xn--bcher-kva.de WHEN validated THEN xn--bcher-kva.de | green | crates/common/tests/it/domain_idna.rs |
| C11 | GIVEN 123.example.com, 1password.com, localhost, _dmarc.example.com or an A-label name WHEN validated THEN unchanged | already_implemented | crates/common/tests/it/domain_idna.rs |
| C12 | GIVEN lens's app WHEN POST /api/v1/domains/１.１.１.１/runs THEN 400 DOMAIN_INVALID and no run recorded | green | crates/lens/tests/it/idna_domain_routes.rs |
| C13 | GIVEN lens's app WHEN GET /ＥＸＡＭＰＬＥ.com THEN 301 to /example.com | green | crates/lens/tests/it/idna_domain_routes.rs |
| C14 | GIVEN the evidence routes WHEN GET /１.１.１.１/dns THEN 404 | green | crates/web/tests/it/evidence_idna_domain.rs |

C7 (`０x７f.1`) was already refused before the change: its last label `1` is numeric on the raw
input; after the reader a second C7 test (`a.１２３`, `a.０x７f`) reaches the check on the mapped
form, and it is green. C11 is a regression guard. Both first tests are kept.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 (orchestrator) | orchestrator | n/a | n/a |

### Behavioural verification

`netray lens` built from this worktree, `crates/lens/lens.dev.toml`, bound to 127.0.0.1:48711:

```
$ curl -si -X POST 'http://127.0.0.1:48711/api/v1/domains/%EF%BC%91.%EF%BC%91.%EF%BC%91.%EF%BC%91/runs'
HTTP/1.1 400 Bad Request
{"error":{"code":"DOMAIN_INVALID","message":"invalid domain: IP addresses are not accepted as domain input: １.１.１.１"}}
$ curl -si 'http://127.0.0.1:48711/%EF%BC%A5%EF%BC%B8%EF%BC%A1%EF%BC%AD%EF%BC%B0%EF%BC%AC%EF%BC%A5.com'
HTTP/1.1 301 Moved Permanently
location: /example.com
$ curl -si -X POST 'http://127.0.0.1:48711/api/v1/domains/1.1.1.1%20./runs'
HTTP/1.1 400 Bad Request
{"error":{"code":"DOMAIN_INVALID","message":"invalid domain: not a valid domain name: 1.1.1.1 ."}}
```

### Reader

| class | where | finding | outcome |
|---|---|---|---|
| DEFERRED | crates/common/src/domain.rs | UTS #46 bidi rule refuses `_`/digit-led ASCII labels beside an RTL label | logged in the spec's decision log, left |
| AMENDMENT | spec.md decision log | a Unicode-keyed history is orphaned, not reachable under its A-label | decision reworded (affected phase 1, repaired) |
| NIT | crates/web/src/pages/result/mod.rs:164 | a hand-made `POST /bücher.de/run` gets the lower-case hint | left |
| NIT | crates/common/tests/it/domain_idna.rs | c7 never reached the mapped numeric check | second C7 test added |
| NIT | crates/common/tests/it/domain_idna.rs | rustfmt changed the protected file | commit carries `ADLC-Test-Change` |

`crates/web/tests/it/evidence_review.rs` `no_entry_page_link_is_percent_encoded` requested
`/%5Cevil.example.com/dns`; `\` is now refused by the rule, so the page never renders its link. The
test now uses `"`, which passes the rule and must be percent-encoded, keeping its intent.
