---
class: unverified-claim-treated-as-fact
repeat-of: specs/solutions/2026-10-09-one-line-bump-estimate.md
---
# A geometric claim over every input was written from seven fixtures

**What failed.** v2-spectrum-phone amended SDD §5.5 with "a run clear at the desktop placement is clear at the phone", then "neither case draws worse than the scaled desktop placement did". Both rested on the seven history fixtures (`crates/web/tests/it/spectrum_phone.rs` c3). Two readers with scratch copies of `place_on` sampled about 90,000 line sets: clusters dense only at the phone (Perms, COOP, MX, DKIM) and two edge-shifted boxes (MTA-STS, TLS-RPT) cross where the desktop is clear, and DNSSEC, DNSKEY, Apex, HTTPS RR cross at two leaders where the scaled drawing crossed at one.
**What worked.** Narrowing the SDD to what holds: no run the scaled desktop drawing kept clear crosses at the phone (no counterexample in the sample), the fixtures are clear, and the two exemptions reach further at the phone geometry.
**How to notice next time.** A sentence quantified over every run that is backed by a fixture loop, not a property test, is a claim, not a fact.
**Check to build.** A proptest in `crates/web/tests/it/spectrum_phone.rs` over random 57-line runs: a run whose desktop placement scaled to the phone plate draws no crossing draws none at the phone placement.
