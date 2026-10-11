
## d1a29bd..94050df

### Reader

COUNTS blockers=1 majors=1 minors=2
LENSES Engineering, Testing
BLOCKER | crates/http/src/module.rs:178 | When spectra reports a check as `skip` with no message, the HTTP module labels it `not_applicable` with the finding "not reported", the text for a check spectra never sent | A site that sets no cookies: spectra emits `cookie_secure` as skip with no message (crates/http/src/quality/checks.rs:95-102); translate prints `http.cookie_secure NotApplicable ["not reported"]`
MAJOR | site/guide/lens.html:100 | The public lens guide still says a warn earns half its weight "rounded down"; a warn now earns exactly half | weight-5 warn: engine 2.5, CheckList "2.5/5", guide 2
MINOR | crates/lens/CLAUDE.md:5 | SCORING SYNC RULE: the README Scoring section changed in a later commit than the scoring switch | a99f408 changes score.rs, README follows in 618fe56
MINOR | justfile:51 | `crates/ip/tests/module_v3.rs` (C2) is never run by the gate | test-rust-offline runs `cargo test -p netray-ip --lib` only

```quote crates/http/src/module.rs:178
                                .unwrap_or_else(|| NOT_REPORTED.to_string()),
```

```quote site/guide/lens.html:100
      <li><strong>warn</strong> &rarr; earns half weight (rounded down)</li>
```

```quote crates/lens/CLAUDE.md:5
- **SCORING SYNC RULE**: Any change to the scoring algorithm (`src/score.rs`, `netray_engine::score`),
```

```quote justfile:51
    cargo test -p netray-ip --lib
```

### Calibration

| finding | refuter | confidence | result | command |
|---|---|---|---|---|
| http skip without message → "not reported" | CONFIRMED (cosmetic) | 9 | held | read crates/http/src/module.rs:173-178 and crates/http/src/quality/checks.rs:95-102 |
| guide "rounded down" | CONFIRMED | 9 | held | read site/guide/lens.html:100 vs crates/engine/src/scoring.rs |

### Summary

Before refutation 1/1/2, after 1/1/2; verified 2, held 2. Fixed on this branch; the fix range is read again below.

## 94050df..8328175

### Reader

COUNTS blockers=0 majors=0 minors=0
LENSES Engineering, Testing

The fix range gives no wrong result: a spectra skip without a message reads "not applicable" while unsent checks keep "not reported"; the guide's "exactly half" matches `crates/engine/src/scoring.rs`; the sitemap matches its generator; the three offline netray-ip test targets exist and need no GeoIP data. Not counted: e6c3eff adds its test together with the fix, without a failing-first `ADLC-Baseline` commit.

### Summary

0/0/0; nothing to refute or verify.
