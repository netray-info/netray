# Review: v2-spectrum-phone

## d6589ca4..c39be449

### Reader

COUNTS blockers=0 majors=1 minors=0
LENSES Engineering, Testing
MAJOR | crates/web/src/spectrum.rs:518 | Some runs are clear at the desktop placement but now show more leader crossings at 480 px or narrower than the drawing before this change did. The phone placement drops to the line-order fallback (spectrum.rs:181) on clusters that are dense only at the phone. That contradicts the amended SDD §5.5 (v2.md:830): "neither case draws worse than the scaled desktop placement did". | Flag only dns.dnssec, dns.dnskey_algorithm, dns.cname_apex and dns.https_svcb; every other check passes. Phone placement: one cluster, clear needs 4 rows, line order 2, line order used: DNSSEC (row 0) covers DNSKEY's leader (row 1), Apex (row 0) covers HTTPS RR's leader (row 1): 2 crossings. Desktop: no crossing; drawn at 358 px as before the change: 1 crossing (DNSKEY covers Apex's leader). 4,261 such desktop-clear runs among about 88k sampled line sets; none where the old drawing was fully clear and the phone placement is not.

```quote crates/web/src/spectrum.rs:518
        let phone = place_phone(&line_inputs)?;
```

```quote crates/web/src/spectrum.rs:181
    if rows_of(&clear) > MIN_ROWS.max(rows_of(&greedy)) {
```

```quote specs/sdd/v2.md:830
   draws worse than the scaled desktop placement did.
```

### Spec reader

COUNTS blockers=0 majors=0 minors=0
REQUIREMENTS met=4 partial=0 missed=0 contradicted=0

No findings. R1 met (place_phone at the 358 px plate, pitch plate/n, phone caption, same rule; RunLayout carries phone_labels and phone_rows). R2 met (template custom properties, phone media query overrides with !important, slim band without phone style, desktop unchanged). R3 met (c3 over all 7 history fixtures; SDD amended in c39be449). R4 met on the report's record (390 and 1280 px captures, design-check 0 deviations).

### Refuted

None.

### Calibration

| finding | refuter answer | confidence | outcome | command |
|---|---|---|---|---|
| MAJOR spectrum.rs:518 phone fallback draws more crossings than the scaled desktop placement | CONFIRMED | 7 | held | scratch test in spectrum_phone.rs (removed): DNSSEC, DNSKEY, Apex, HTTPS RR warn: desktop none, scaled 1 (DNSKEY covers Apex), phone 2 (DNSSEC covers DNSKEY, Apex covers HTTPS RR) |

### Roll call

| id | answer | evidence |
|---|---|---|
| P03 | absence | no status enum, no codename in the changed templates or sources |
| P10 | absence | label placement only; no verdict or scoring path changes |
| P12 | absence | no outbound call or module run |
| P13 | absence | no route, no limiter change |
| P18 | absence | no client, no config struct |
| P26 | absence | no new check ID |
| P35 | convergence | the SDD 5.5 amendment arrives with spectrum_phone.rs and the phone proptest |
| P36 | convergence | island fixtures change with the template and component they render; tokens.json untouched |
| P40 | absence | no module added or removed; crates/engine untouched |

### Summary

Reader: 0/1/0 before and after refutation (the MAJOR held: the SDD sentence "neither case draws worse than the scaled desktop placement did" overstates; it holds as clear or not, not by count). Spec reader: 0/0/0, REQUIREMENTS met=4 partial=0 missed=0 contradicted=0. verified 1, held 1. Roll call: answered 9, convergence 2, divergence 0, absence 7.
