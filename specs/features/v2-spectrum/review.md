# Review: v2-spectrum

## 8bde1a817303216beca0573460ede67096f808ae..f8316036dc9fd06e9cf64a2d940d428d988a762f

### Reader

COUNTS blockers=0 majors=0 minors=2
LENSES Engineering, Security, Testing
MINOR | crates/web/static/spectrum.css:3 | The new `overflow-x: auto` makes `.spectrum` scroll sideways at container widths where the pitch is 8 px or more, which goes against R26. The cause is the `ip` group name: at `left: 98.1481%`, and now at `--font-size-label`, it runs past the right edge of the box. | Real component from `layout(..., 432)` and `SpectrumView::from_layout`, measured in headless Chromium with `tokens.css`, `base.css` and `spectrum.css`: at a 600 px container clientWidth 600, scrollWidth 607; at 900 px 900 vs 901; at 358 px scrollWidth 442. The spec puts "the one-line IP group at the right edge" out of scope (v2-result-page), hence MINOR.
MINOR | crates/web/tests/drawing_rules.rs:360 | `spectrum_pass_and_unmeasured_lines_carry_no_short_name_above_the_plate` cannot fail on the claim in its name: the view literal no longer supplies the pass (`MX`) and untested (`SPF`) short names, and the template's tone filter is gone. | If the template rendered every label it is given, this assertion would still pass; the rule stays covered by `spectrum_component.rs` c14 and property claim (b).

```quote crates/web/static/spectrum.css:3
  overflow-x: auto;
```

```quote crates/web/tests/drawing_rules.rs:360
    for short in ["MX", "SPF"] {
```

### Spec reader

COUNTS blockers=0 majors=0 minors=1
REQUIREMENTS met=18 partial=0 missed=0 contradicted=0
MINOR | crates/web/templates/components/spectrum.html:11 | Each placed line label now carries a `data-code` attribute, which no requirement asks for on labels. | A layout with `dns.caa` warn renders `<span class="spectrum__line-label" data-tone="warn" data-code="dns.caa" ...>`; `drawing_rules.rs` `line_tag` now starts its search at `<svg` (ADLC-Test-Change in 7d7df71).

```quote crates/web/templates/components/spectrum.html:11
        <span class="spectrum__line-label" data-tone="{{ label.tone.as_str() }}" data-code="{{ label.code }}" style="left: clamp(0px, {{ label.x_pct() }}%, calc(100% - {{ label.width_px() }}px)); bottom: calc(var(--spectrum-label-leader) + {{ label.row }} * var(--spectrum-label-row))">{{ label.text }}</span>
```

```quote specs/features/v2-spectrum/spec.md:126
    - Inside each band, every run of the layout is one `<g class="spectrum__run" data-run="<k>">` (k = 0 for the current run, then newest first) with that run's tones and heights. The label band likewise holds one `<div class="spectrum__labels" data-run="<k>">` per run with that run's placement.
```

### Refuted

None: no BLOCKER or MAJOR finding, so no refuter ran.

### Calibration

No finding went to the refuter.

### Roll call

| principle | answer | evidence |
|---|---|---|
| P03 | convergence | only new enums `LineTone`, `SectionState` (no status type); no codenames added |
| P10 | convergence | a section with a weighted unmeasured check becomes `Unmeasured`; only measured lines take height |
| P12 | absence | no outbound call or engine module entry |
| P13 | absence | no limiter or route change |
| P18 | absence | no client, no config struct |
| P26 | absence | no new check ID or fetch |
| P35 | absence | no rule changed; SDD changes arrive with tests/repo checks |
| P36 | convergence | tokens.json changed with build.rs-generated values; no hand-edited generated file |
| P40 | convergence | netray-web adds only netray-model and netray-engine; no module crate |

### Summary

Before refutation: reader 0/0/2, spec reader 0/0/1 (REQUIREMENTS met=18 partial=0 missed=0 contradicted=0). After refutation: unchanged, 0 blockers, 0 majors, 3 minors. verified 0, held 0. Roll call: 9 answered, 4 convergence, 0 divergence, 5 absence.
