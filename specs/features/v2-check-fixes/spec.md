# Spec: v2 check fixes

Status: Done
Created: 2026-10-11
Finished: 2026-10-11

## Goal

The five check defects found while writing catalogue v3 are fixed. Each check now does what its `checks.toml` entry already says:

- `http.cors` flags only an echo of the probe's own Origin, or `null`.
- `tls.caa_compliant` fails on the critical flag only for an unknown tag.
- `tls.strong_signature` ignores root self-signatures and no longer passes MD5, MD2, ECDSA-SHA1, DSA-SHA1, OIW SHA-1 or unrecognised algorithms.
- `tls.chain_trusted` no longer depends on the hostname.
- `tls.chain_complete` (and `tls.aia_reachability`, which follows it) no longer passes a leaf sent alone whose issuer is not a trust anchor.

Each fix is proven by a unit test on the scenario the defect names.

## Non-goals

- Fetching intermediates over AIA. `tls.aia_reachability` keeps judging from the served chain.
- Parsing RSASSA-PSS parameters.
- Changing weights. Profile v3 changes only by R8 (one hard-fail entry); v3 is unreleased, so its lock hash moves with it. In `checks.toml` only the `finding` texts of `tls.strong_signature` (R4) and `http.cors` (R1) and the `severity` of `tls.hostname_match` (R8) change.
- Regenerating the stored lens goldens (`tests/fixtures/contracts/lens-*.json`). They are recorded outputs, and none of the scenarios below occurs in them.

## Context and constraints

- Catalogue texts that state the intended behaviour are in `crates/engine/catalogue/checks.toml`, entries `http.cors`, `tls.caa_compliant`, `tls.strong_signature`, `tls.chain_complete`, `tls.aia_reachability` and `tls.chain_trusted` (landed in v2-catalogue, f68ccb2).
- **CORS**
  - The probe sends `Origin: https://evil.example.com`. The literal is at `crates/http/src/inspect/mod.rs:97` and is passed to `execute_request` (`request.rs:64-70`).
  - `analyze_cors` receives only the response headers (`mod.rs:205`). It sets `reflects_origin` for any non-empty ACAO other than `*` (`crates/http/src/inspect/cors.rs:17`).
- **CAA**
  - `CaaRecord.issuer_critical` is set for every tag (`crates/tls/src/dns/caa.rs:71-75`, `from_presentation` :45-58).
  - `check_caa_compliance` fails when any record is critical (`crates/tls/src/validate/caa_compliance.rs:21`).
  - The test `issuer_critical_blocks_all_issuance` (:232) asserts Fail for a critical `issue` record.
- **Signatures**
  - `format_sig_algo` (`crates/tls/src/tls/chain.rs:273-285`) maps sha1/sha256/sha384/sha512WithRSA, ecdsa-with-SHA256/384/512 and Ed25519. It renders md5WithRSA (1.2.840.113549.1.1.4), RSASSA-PSS (1.2.840.113549.1.1.10), ecdsa-with-SHA1 (1.2.840.10045.4.1) and Ed448 (1.3.101.113) as `Unknown(<oid>)`.
  - `weakest_signature` is the minimum `sig_strength` over every certificate, roots included (`crates/tls/src/validate/chain_trust.rs:26-31`, :137-149).
  - `check_strong_signature` fails only on "sha1"/"md5" in the name (`crates/tls/src/quality/checks.rs:114-132`), so `Unknown(...)` passes.
  - A certificate x509-parser rejects gets `signature_algorithm` "" and `is_self_signed` false (`chain.rs:64`, :69).
  - `CertInfo` carries `position` (`leaf`, `leaf_self_signed`, `intermediate`, `root`) and `is_self_signed` (`chain.rs:10-35`, :188-202).
- **Trust**
  - `verify_chain_trust` calls `WebPkiServerVerifier::verify_server_cert` with the server name (`chain_trust.rs:55-90`; verifier `crates/tls/src/state.rs:44`). It returns untrusted for a name mismatch and for a target without a hostname.
  - rustls 0.23.45 exports `rustls::client::verify_server_cert_signed_by_trust_anchor`, which verifies the chain without a name. The trust store is `AppState.trust_store` (`state.rs:22`, :47); `inspect_port` passes only the verifier on to `validate_chain` (`crates/tls/src/routes.rs:1075`, :1095).
- **Chain completeness**
  - `check_chain_order` returns true for a chain of length ≤ 1 (`chain_trust.rs:105-112`).
  - `check_chain_complete` gives Pass when the order is correct, Warn when the order is wrong but the chain is trusted, and Fail otherwise (`checks.rs:91-112`).
  - `check_aia_reachability` passes when the chain is complete and otherwise gives Warn with a `ca_issuers_url` and Fail without one (`checks.rs:685-736`).
  - The module maps AIA to NotApplicable when the chain is complete (`crates/tls/src/module.rs:299-310`).
- **Fixtures that must stay green**
  - The fixtures in `crates/tls/tests/module_v3.rs`, `port_results_table.rs` and `contract_golden.rs` use a single leaf that is not self-signed, with `chain_trusted: true`. They store `ValidationResult` values (`contract_golden.rs:110`, `port_results_table.rs:87`, `tests/fixtures/contracts/tlsight-inspect.json:61`); `validate_chain` runs only at `routes.rs:1095`. Their expectations stay as they are.
- **webpki error ranking.** For an expired leaf with an unknown issuer, webpki reports the expiry, not the unknown issuer: it checks the end entity's validity before any issuer lookup (`rustls-webpki-0.103.15/src/verify_cert.rs:63`), and `UnknownIssuer` ranks lowest (`src/error.rs:350`). A trust error therefore cannot tell whether a chain is complete.
- **Catalogue rule.** A catalogue text describes every warn and fail branch of its check (`CLAUDE.md`, catalogue-text rule).
- **Gate and commits**
  - The gate is `just adlc-verify`, offline (`justfile`, `test-rust-offline`).
  - A changed protected test needs `ADLC-Test-Change: <path> -- <reason>`.
  - Public commits use neutral wording.

## Requirements

1. **CORS echo.** The probe Origin is one constant in `crates/http` used both by the request and by `analyze_cors`. `reflects_origin` is true only when ACAO equals that constant byte for byte (as the Fetch Standard's CORS check compares) or is `null` (a sandboxed or opaque origin any page can obtain). With credentials it fails as today. The catalogue `finding` of `http.cors` reads: "The site echoes an untrusted request Origin in Access-Control-Allow-Origin, or answers with Access-Control-Allow-Origin: null, together with Access-Control-Allow-Credentials: true, or allows any origin with Access-Control-Allow-Origin: *." An ACAO naming any other specific origin, with or without credentials, passes with "CORS configured". The `*` cases are unchanged.
2. **CAA critical flag.** The known CAA tags are `issue`, `issuewild`, `iodef`, `contactemail`, `contactphone`, `issuemail` and `issuevmc`, compared ASCII case-insensitively. Only a critical record whose tag is not known fails `tls.caa_compliant`. A critical record with a known tag is evaluated like a non-critical one.
3. **Signature algorithms.** `format_sig_algo` maps 1.2.840.113549.1.1.4 to `md5WithRSAEncryption`, 1.2.840.113549.1.1.2 to `md2WithRSAEncryption`, 1.2.840.10045.4.1 to `ecdsa-with-SHA1`, 1.2.840.10040.4.3 to `dsa-with-SHA1`, 1.3.14.3.2.29 to `sha1WithRSASignature` and 1.3.101.113 to `Ed448`.
4. **Signature verdict.**
   - `tls.strong_signature` considers every served certificate except a self-signed certificate that is not the leaf (a root's self-signature).
   - Among the considered certificates, a SHA-1, MD5 or MD2 signature fails.
   - Otherwise any `Unknown(<oid>)` algorithm gives Warn with the message `unrecognised signature algorithm <oid>`, and a certificate whose algorithm could not be read (empty string) gives Warn with `unreadable certificate signature`.
   - Otherwise the check passes.
   - A Fail outranks a Warn regardless of where the certificates sit in the chain.
   - The catalogue `finding` of `tls.strong_signature` reads: "The leaf or an intermediate certificate is signed with SHA-1, MD5 or MD2, or with an algorithm netray does not recognise; root self-signatures are ignored." `impact`, `fix_hint` and the other fields stay.
   - The API field `validation.weakest_signature` excludes root self-signatures too: it is the value the check reads.
   - The check's explanations name the same scope: tlsight's `ValidationSummary.tsx`, `crates/tls/docs/quality.md` and lens's `checkMeta.ts` description.
5. **Trust without the name.** `chain_trusted` reports whether the served chain builds to a trust anchor in the store at the current time, using `verify_server_cert_signed_by_trust_anchor` with no server name. A hostname mismatch no longer makes it false (`tls.hostname_match` keeps reporting the mismatch). A target without a hostname, such as an IP address, gets a real trust verdict.
6. **Chain completeness.** Completeness is decided from the served certificates and the trust store's anchors, not from the trust verdict or its error.
   - A chain in correct order passes `tls.chain_complete` when a served certificate is self-signed, or when a served certificate or its issuer is a trust anchor in the store (compared by subject name); certificates served past that point do not count. The Pass detail says whether the chain reaches an anchor or ends at a self-signed certificate.
   - Otherwise a correctly ordered chain fails, including a leaf sent alone whose issuer is not an anchor, whether or not the leaf is expired.
   - Wrong order: trusted gives Warn and untrusted gives Fail, as today.
   - `tls.aia_reachability` follows from this without changes to its own rules.

7. **CAA for wildcard certificates.** When the leaf carries a wildcard SAN and the CAA set holds `issuewild` records, those records decide whether the issuing CA is authorised for it, ignoring `issue` (RFC 8659 §4.3); without `issuewild` records `issue` decides as today. An empty `issuewild` value (`;`) authorises no CA. The leaf's non-wildcard SANs stay judged by `issue`; the check passes only when every applicable set authorises the CA.

8. **A name mismatch stays a hard fail.** `tls.hostname_match` joins the TLS `hard_fail` list of profile v3, and its catalogue `severity` becomes `critical` (the header rule: a hard fail is critical). Every hard-fail check of the profile carries severity `critical` in the catalogue. An IP target, where `hostname_match` is not applicable, is unaffected.

## Phases

## Phase 1 — Independent check fixes

**Depends on:** none
**Requirements:** 1, 2, 3, 4, 7

`crates/http/src/inspect/{mod.rs, cors.rs}`; `crates/tls/src/validate/caa_compliance.rs` (known-tag set); `crates/tls/src/tls/chain.rs` (`format_sig_algo`); `crates/engine/catalogue/checks.toml` (the one `finding`); `crates/tls/src/validate/chain_trust.rs` and `crates/tls/src/quality/checks.rs` (signature verdict over non-root certificates). `issuer_critical_blocks_all_issuance` changes to an unknown critical tag (ADLC-Test-Change).

### Test Scenarios

GIVEN a probe response with ACAO `https://evil.example.com` and ACAC `true` WHEN analyze_cors runs THEN `reflects_origin` is true and status is Fail.
GIVEN ACAO `null` and ACAC `true` WHEN analyze_cors runs THEN `reflects_origin` is true and status is Fail.
GIVEN ACAO `HTTPS://EVIL.EXAMPLE.COM` and ACAC `true` WHEN analyze_cors runs THEN `reflects_origin` is false and status is Pass.
GIVEN ACAO `https://app.example.com` and ACAC `true` WHEN analyze_cors runs THEN `reflects_origin` is false and status is Pass.
GIVEN ACAO `*` without credentials WHEN analyze_cors runs THEN status is Warn (unchanged).
GIVEN the probe request WHEN its Origin header is read THEN it equals the constant analyze_cors compares against.
GIVEN a critical CAA record with tag `foo` WHEN check_caa_compliance runs THEN Fail.
GIVEN a critical `issue` record authorising the leaf's CA WHEN check_caa_compliance runs THEN Pass.
GIVEN a critical `issue` record not authorising the leaf's CA WHEN check_caa_compliance runs THEN Fail.
GIVEN a critical `IODEF` or `contactemail` record next to an `issue` record authorising the CA WHEN check_caa_compliance runs THEN Pass.
GIVEN OIDs 1.2.840.113549.1.1.4, 1.2.840.113549.1.1.2, 1.2.840.10045.4.1, 1.2.840.10040.4.3, 1.3.14.3.2.29, 1.3.101.113 WHEN format_sig_algo runs THEN `md5WithRSAEncryption`, `md2WithRSAEncryption`, `ecdsa-with-SHA1`, `dsa-with-SHA1`, `sha1WithRSASignature`, `Ed448`.
GIVEN an intermediate signed with `dsa-with-SHA1` WHEN strong_signature is assessed THEN Fail.
GIVEN a certificate with signature algorithm "" WHEN strong_signature is assessed THEN Warn with `unreadable certificate signature`.
GIVEN the catalogue WHEN `tls.strong_signature` is loaded THEN its `finding` names MD2 and unrecognised algorithms.
GIVEN the catalogue WHEN `http.cors` is loaded THEN its `finding` names `null`.
GIVEN an intermediate signed with `md2WithRSAEncryption` WHEN strong_signature is assessed THEN Fail.
GIVEN a wildcard leaf from the CA in `issue`, with `issuewild ";"` WHEN check_caa_compliance runs THEN Fail.
GIVEN a wildcard leaf, `issue` naming another CA and `issuewild` naming the leaf's CA WHEN check_caa_compliance runs THEN Pass.
GIVEN a wildcard leaf from the CA in `issue` and no `issuewild` WHEN check_caa_compliance runs THEN Pass (unchanged).
GIVEN a non-wildcard leaf from the CA in `issue`, with `issuewild ";"` WHEN check_caa_compliance runs THEN Pass.
GIVEN a leaf and intermediate signed with SHA-256 and a root self-signed with SHA-1 WHEN strong_signature is assessed THEN Pass.
GIVEN an intermediate signed with ecdsa-with-SHA1 WHEN strong_signature is assessed THEN Fail.
GIVEN a self-signed leaf (`leaf_self_signed`) signed with SHA-1 WHEN strong_signature is assessed THEN Fail.
GIVEN a leaf signed with `Unknown(1.2.840.113549.1.1.10)` WHEN strong_signature is assessed THEN Warn with `unrecognised signature algorithm 1.2.840.113549.1.1.10`.
GIVEN a leaf with an `Unknown(...)` algorithm and an intermediate with SHA-1 WHEN strong_signature is assessed THEN Fail.

## Phase 2 — Trust and chain completeness

**Depends on:** Phase 1
**Requirements:** 5, 6, 8

`crates/tls/src/validate/chain_trust.rs` (trust via `verify_server_cert_signed_by_trust_anchor`; completeness from the anchors' subjects); `crates/tls/src/routes.rs` (pass the trust store to `validate_chain`); `crates/tls/src/quality/checks.rs` (`check_chain_complete`); `crates/tls/src/state.rs` only if the store or the supported algorithms must be passed differently.

### Test Scenarios

GIVEN a chain that builds to a trust anchor, inspected under a hostname the leaf does not cover WHEN trust is verified THEN `chain_trusted` is true and `hostname_match` is Fail.
GIVEN the same chain inspected with no hostname WHEN trust is verified THEN `chain_trusted` is true.
GIVEN a self-signed leaf not in the store WHEN trust is verified THEN `chain_trusted` is false (unchanged).
GIVEN a single leaf, not self-signed, whose issuer is not an anchor in the store WHEN chain_complete is assessed THEN Fail, and aia_reachability is Fail without and Warn with a caIssuers URL.
GIVEN the same lone leaf, expired WHEN chain_complete is assessed THEN Fail.
GIVEN a single leaf, not self-signed, whose issuer is an anchor in the store WHEN chain_complete is assessed THEN Pass and aia_reachability is NotApplicable.
GIVEN a correctly ordered leaf + intermediate whose intermediate's issuer is an anchor, with the leaf expired WHEN chain_complete is assessed THEN Pass.
GIVEN a correctly ordered chain ending in a self-signed root that is not in the store WHEN chain_complete is assessed THEN Pass.
GIVEN a wrongly ordered trusted chain WHEN chain_complete is assessed THEN Warn (unchanged).
GIVEN profile v3 WHEN its hard-fail checks are listed THEN they are `tls.chain_trusted`, `tls.hostname_match`, `tls.not_expired`, `tls.tls_reachable`, each with catalogue severity `critical`.
GIVEN the fixtures of `crates/tls/tests/{module_v3, port_results_table, contract_golden}.rs` WHEN the suite runs THEN their asserted statuses hold unchanged; a new `ValidationResult` field may be added to their data (the fixture's lone leaf is trusted, so its issuer is an anchor), with an ADLC-Test-Change trailer.

## Decision log

- Same-origin echo over any non-`*` ACAO for `http.cors`: the catalogue says "echoes an untrusted request Origin", and the probe sends a known one (from the catalogue text, 2026-10-11).
- `null` counts as an echo and the match is byte for byte: a sandboxed iframe sends `Origin: null`, and browsers compare ACAO exactly (independent reading, 2026-10-11).
- Known CAA tags RFC 8659 plus `contactemail`, `contactphone`, `issuemail`, `issuevmc` over RFC 8659 alone: CABF-conformant zones are not punished (operator, 2026-10-11).
- An unknown signature algorithm gives Warn over Fail or Pass: half the points and a visible finding, without failing legitimate PSS or Ed448 chains; RSASSA-PSS is not parsed (operator, 2026-10-11).
- Extend the `tls.strong_signature` finding over keeping the catalogue byte-identical: the catalogue-text rule requires a text for the new Warn branch (operator, 2026-10-11).
- Trust-anchor API over matching the name error: IP targets get a real trust verdict (operator, 2026-10-11).
- A chain is complete when it ends at an anchor or self-signed, over failing every lone leaf: a leaf issued directly by a root is not punished (operator, 2026-10-11).
- Completeness from the anchors' subjects over the trust verdict or webpki's error kind: webpki reports expiry before an unknown issuer, so an expired lone leaf would pass. Consequence: a private-CA chain without its root also fails completeness, besides the hard-fail `chain_trusted` (operator, 2026-10-11).
- Phase 1 reader amendments repaired in phase over noting them for later: the `http.cors` finding names `null`, R4's finding names MD2, `weakest_signature` excludes roots in the API too, the explanation texts follow (operator, 2026-10-11).
- `issuewild` evaluated in this feature (R7) over a later one: the blanket critical Fail used to hide a critical `issuewild ";"`, so R2 alone would regress it (operator, 2026-10-11).
- Completeness over every served certificate, not only the last: a trusted chain that also serves cross-certificates past its anchor is complete (phase 2 reading, 2026-10-11).
- `tls.hostname_match` becomes a hard fail over leaving the mismatch at 10 points: R5 removed the name from `chain_trusted`, which was the only hard fail a name mismatch reached, and browsers block such a site (branch reading, operator, 2026-10-11).
- A separate feature before 0.25.0 over folding into v2-catalogue (operator, 2026-10-11).

## Open decisions

None.

## Out of scope

- The R4 review table and the grade shift against the V1 baseline: handed to the operator after this feature, before `just release 0.25.0` (planning repo `specs/sdd/v2.md` §3.9).
- `unmeasured`, definite negatives, error skips: feature `v2-unmeasured`.
- Evidence shapes: feature `v2-evidence`.
