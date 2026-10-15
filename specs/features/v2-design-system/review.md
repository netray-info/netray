## 05e7d64..ee21507

### Reader

COUNTS blockers=2 majors=0 minors=1
LENSES Engineering, Security, Testing
BLOCKER | crates/web/src/gate.rs:483 | Gate rule 3 reports a `<script>` whose `src` is an askama expression as an inline script. The scanner turns `{{ … }}` into spaces at line 426 before this check, so the src value trims to empty and `has_src` comes out false. Requirement 6 only refuses scripts "with no `src` attribute", and says askama expressions are opaque. | I ran an isolated copy of `gate.rs`, `build.rs` and `tokens.json` (built in the scratchpad, not the tree). `templates/x.html` containing `<script type="module" src="{{ asset }}"></script>` gives `r3 templates/x.html:1 inline \`<script>\``; `src="{{- asset -}}"` gives the same. Only a literal `src="/x.js"` has a passing test row in `tests/gate.rs:386`.

```quote crates/web/src/gate.rs:483
            let has_src = tag.values("src").any(|(_, v)| !v.trim().is_empty());
```

BLOCKER | crates/web/src/gate.rs:71 | Gate rule 1 accepts a CSS named colour in any colour-taking property missing from `COLOUR_PROPERTIES`. Requirement 6 refuses a named colour anywhere in a stylesheet, and the same gate refuses the hex form of the same value. | Same isolated copy: `static/x.css` with `.a { mask-image: linear-gradient(to right, black 90%, transparent); }` gives no violations. Swapping in `#000` gives `colour literal \`#000\` in \`mask-image\``, and `black` under `background-image` is refused. Named colours also pass in `-webkit-mask-image`, `-webkit-tap-highlight-color: red`, `-webkit-text-stroke: … black`, `backdrop-filter: drop-shadow(0 0 4px black)`, `text-emphasis: filled red` and `list-style-image: linear-gradient(red, blue)`.

```quote crates/web/src/gate.rs:71
const COLOUR_PROPERTIES: [&str; 18] = [
```

MINOR | crates/web/tests/drawing_rules.rs:262 | The test claims "none for pass or unmeasured", but it cannot fail on that: its fixture gives the pass and unmeasured lines `label: None` (lines 81, 85). The spectrum macro renders any line's label whatever its tone (`templates/components/spectrum.html:5`). | `SpectrumLine { tone: Tone::Pass, label: Some("MX".into()), .. }` renders `<span class="spectrum__line-label" data-tone="pass">MX</span>` above the plate. That breaks the requirement-7 drawing rule (only warn, fail and fixed short names above the plate), and this test still passes.

```quote crates/web/tests/drawing_rules.rs:262
        "warn, fail and fixed only; none for pass or unmeasured: {above}"
```

### Refuted

None.

### Calibration

| Finding | Refuter | Confidence | Outcome | Command |
|---|---|---|---|---|
| `gate.rs:483` askama `src` read as inline script | CONFIRMED | 9 | held | scratch crate on `netray_web::gate::check`: `<script type="module" src="{{ asset }}"></script>` → `r3 inline <script>` |
| `gate.rs:71` named colour outside `COLOUR_PROPERTIES` passes | CONFIRMED | 8 | held | same crate: `mask-image: linear-gradient(to right, black 90%, transparent)` → no violation; `-webkit-tap-highlight-color: red` → none; `#000` in `mask-image` → `r1 colour literal` |

### Roll call

| Principle | Answer | Evidence |
|---|---|---|
| P03 | convergence | no codename or `*Status`/`*Verdict` enum in the added templates, SVGs or code |
| P10 | absence | no check or scoring path changed |
| P12 | absence | no outbound client or engine call |
| P13 | absence | no route, no limiter; `test_web_crate.sh` holds that |
| P18 | absence | no client or config struct added |
| P26 | absence | no check ID emitted |
| P35 | convergence | the gate (R45) ships with its enforcing tests |
| P36 | convergence | `tokens.css` and constants generated into OUT_DIR from `tokens.json`, none committed |
| P40 | absence | no check module added |

### Summary

Before refutation 2/0/1, after 2/0/1; verified 2, held 2. Roll call 9 answered, 3 convergence, 0 divergence, 6 absence. Both blockers hold: the review stops with `blocker`.

## ee21507..c720bfc

### Reader

COUNTS blockers=2 majors=0 minors=0
LENSES Engineering, Security, Testing

BLOCKER | crates/web/src/gate.rs:619 | Gate rule 1 now refuses CSS that has no colour in it. The change treats every property outside `IDENTIFIER_PROPERTIES` as able to hold a colour. That list has the longhands `transition-property` and `animation-name` but not their shorthands `transition` and `animation`, so an identifier that is also a named or system colour gets reported as a colour literal there. | Isolated copy of `gate.rs` at c720bfc: `.a { transition: background 150ms ease; }` gives `r1 static/x.css:1 colour literal \`background\` in \`transition\``, while `.a { transition-property: background; }` passes. `.a { animation: highlight 1s ease-in-out; }` gives `colour literal \`highlight\` in \`animation\``. The gate at ee21507 passes both. The new "colour-like identifiers" test row checks only the longhands.

```quote crates/web/src/gate.rs:619
    prop.starts_with("--") || !IDENTIFIER_PROPERTIES.contains(&unprefixed(prop))
```

BLOCKER | crates/web/src/gate.rs:513 | An unquoted askama `src` is still reported as an inline script. For `src={{ asset }}`, `parse_attrs` skips the blanked expression as whitespace and returns an empty value at the tag's `>` offset. That offset equals the askama span's exclusive end, so `at < e` is false and `has_src` stays false. Requirement 6 refuses only scripts "with no `src` attribute" and says askama expressions are opaque. | `<script src={{ asset }}></script>` or `<script type="module" src={{ asset }}></script>` gives `r3 templates/x.html:1 inline \`<script>\``. The quoted form `src="{{ asset }}"` passes. Only quoted forms have test rows.

```quote crates/web/src/gate.rs:513
                !v.trim().is_empty() || askama.iter().any(|&(s, e)| s < at + v.len() && at < e)
```

### Refuted

None.

### Calibration

| Finding | Refuter | Confidence | Outcome | Command |
|---|---|---|---|---|
| `gate.rs:619` `transition`/`animation` shorthands refused | CONFIRMED | 9 | held | scratch crate on `netray_web::gate::check`: `transition: background 150ms ease` → `r1 colour literal background`; `animation: highlight 1s ease-in-out` → `r1 colour literal highlight` |
| `gate.rs:513` unquoted askama `src` read as inline | CONFIRMED | 9 | held | same crate: `<script src={{ asset }}></script>` → `r3 inline <script>` |

### Roll call

Not repeated: the range touches only `gate.rs`, `spectrum.html` and their tests; the roll call over 05e7d64..ee21507 stands.

### Summary

Before refutation 2/0/0, after 2/0/0; verified 2, held 2. Both blockers hold: the review stops with `blocker`; fixed on the branch by the operator's standing decision.

The review record for ee21507..c720bfc carries `--tokens 113000 --seconds 270`, an estimate entered before the refuter's usage arrived; the agents reported 111323 tokens (reader 86687, refuter 24636) and 264 s. Records are not edited.

## c720bfc..528dd43

### Reader

COUNTS blockers=0 majors=0 minors=2
LENSES Engineering, Security, Testing

MINOR | crates/web/src/gate.rs:520 | The new unquoted-src branch accepts any askama span between `src=` and `>`, including a `{# … #}` comment that renders nothing, so `<script src={# cdn later #}>alert(1)</script>` (rendering `<script src=>alert(1)</script>`) passes rule 3; c720bfc refused it. Minor: deliberate evasion is out of scope, and the quoted `src="{# … #}"` already passed. | Isolated copies of `gate.rs` at both commits.

```quote crates/web/src/gate.rs:520
                            || (e <= at
```

MINOR | crates/web/tests/gate.rs:667 | The row "unquoted askama script src with other attributes passes" passes without the new branch: `parse_attrs` skips the blanked `{{ asset }}` and reads `defer` as the value of `src`. The same misparse lets `<script src={{ asset }} onload="init()">` and `<button class={{ c }} onclick="go()">` pass rule 3 at both commits. | Isolated copy at c720bfc without the branch.

```quote crates/web/tests/gate.rs:667
            "<script type=\"module\" src={{ asset }} defer></script>\n",
```

### Refuted

None: no BLOCKER or MAJOR to refute.

### Calibration

No finding went to the refuter.

### Roll call

Not repeated: the range touches only `gate.rs` and its test.

### Summary

Before and after refutation 0/0/2; verified 0, held 0. The two minors stay open: an unquoted askama value directly followed by an event-handler attribute is misparsed as that attribute's value, so `<button class={{ c }} onclick="go()">` passes rule 3; listed for the pages phase, where templates first use unquoted askama attributes.

The review record for c720bfc..528dd43 carries `--tokens 0 --seconds 0`, entered before the reader's usage arrived; the reader reported 82276 tokens and 214 s. Records are not edited.
