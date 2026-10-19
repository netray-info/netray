# Plan: v2-http-checks

## Phase 1 — Check fixes: Referrer-Policy lists, inspection duration, Set-Cookie rows

### Groups

G1: C4, C17, C18 · G2: C1, C6-C10 · G3: C2, C3, C11-C16 · G4: C5 (SDD, prose)

### Plan

- G1: `crates/model/src/canonical.rs` `table`: a volatile row path also drops rows keyed `<row>#<digits>` (helper `row_covers`); fingerprint and diff both read the canonical form.
- G2: `crates/http/src/inspect/security.rs` Referrer-Policy scorer: last known comma-separated token decides; "Permissive policy: <token>"; "Unrecognized value: <value>". Feeds V1 quality check and `http.referrer_policy`.
- G3: `crates/http/src/module.rs` `REGISTER`, `VOLATILE`, `server_block` (`inspection_ms`), `headers_block` (`set-cookie`, `set-cookie#<n>` rows); `crates/lens/src/modules.rs` `HttpServer` reads `inspection_ms`; `crates/http/tests/evidence.rs` volatile list (test change); lens full goldens regenerate with `UPDATE_GOLDEN=1 cargo test -p lens --test lens_full_output`.
- G4: `specs/sdd/v2.md` §3.7 (done with the code commit).
