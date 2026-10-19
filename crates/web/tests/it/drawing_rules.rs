//! Drawing rules of the spectrum and the trace (V2 spec v2-design-system, requirement 7, Phase 3;
//! SDD §5.4 and §5.6): rendered markup plus the component stylesheet must resolve each mark to
//! the token its state owns.
use netray_web::components::{
    SpectrumBand, SpectrumBandRun, SpectrumLabel, SpectrumLabelSet, SpectrumLine, SpectrumRegion,
    SpectrumView, Tone, TraceRow, TraceView, spectrum, trace,
};
use netray_web::tokens::CSS as TOKENS;
use std::fs;
use std::path::PathBuf;

fn stylesheet(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("static")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing file: {}", path.display()))
}

/// The last value `prop` takes over all rules of `css` whose selector list contains one of
/// `selectors` (whitespace ignored), in source order; `None` when no rule sets it.
fn resolve(css: &str, selectors: &[&str], prop: &str) -> Option<String> {
    let squash = |s: &str| -> String { s.chars().filter(|c| !c.is_whitespace()).collect() };
    let wanted: Vec<String> = selectors.iter().map(|s| squash(s)).collect();
    let mut found = None;
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        let prelude = &rest[..open];
        let body = &rest[open + 1..open + close];
        if prelude
            .split(',')
            .any(|s| wanted.iter().any(|w| *w == squash(s)))
        {
            for decl in body.split(';') {
                if let Some((k, v)) = decl.split_once(':')
                    && k.trim() == prop
                {
                    found = Some(v.trim().to_string());
                }
            }
        }
        rest = &rest[open + close + 1..];
    }
    found
}

/// Every opening tag of `tag` (`<line`, `<rect`, ...) with its attribute text, in document order.
fn tags<'a>(html: &'a str, tag: &str) -> Vec<&'a str> {
    let needle = format!("<{tag} ");
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find(&needle) {
        let end = rest[i..].find('>').expect("unterminated tag") + i;
        out.push(&rest[i..end]);
        rest = &rest[end..];
    }
    out
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!(" {name}=\"");
    let i = tag.find(&needle)? + needle.len();
    Some(&tag[i..i + tag[i..].find('"')?])
}

fn line(code: &str, x: f64, tone: Tone) -> SpectrumLine {
    SpectrumLine {
        code: code.into(),
        x,
        height: 50.0,
        tone,
    }
}

fn band(protocol: &str, boundary: Option<f64>, lines: Vec<SpectrumLine>) -> SpectrumBand {
    SpectrumBand {
        protocol: protocol.into(),
        boundary,
        runs: vec![SpectrumBandRun {
            run: 0,
            shown: true,
            unmeasured: false,
            lines,
        }],
    }
}

fn label(code: &str, text: &str, tone: Tone, x: f64) -> SpectrumLabel {
    SpectrumLabel {
        code: code.into(),
        text: text.into(),
        tone,
        row: 0,
        x,
        width: 40.0,
    }
}

fn label_set(labels: Vec<SpectrumLabel>) -> Vec<SpectrumLabelSet> {
    vec![SpectrumLabelSet {
        run: 0,
        shown: true,
        labels,
    }]
}

fn fixed_view() -> SpectrumView {
    SpectrumView {
        label: "Findings by position".into(),
        line_count: 5,
        rows: 1,
        bands: vec![
            band(
                "dns",
                None,
                vec![
                    line("DNS-001", 5.0, Tone::Pass),
                    line("DNS-002", 15.0, Tone::Warn),
                ],
            ),
            band(
                "tls",
                Some(50.0),
                vec![
                    line("TLS-001", 55.0, Tone::Fail),
                    line("TLS-002", 65.0, Tone::Fix),
                    line("TLS-003", 75.0, Tone::Untested),
                ],
            ),
        ],
        label_sets: label_set(vec![
            label("DNS-002", "SPF", Tone::Warn, 15.0),
            label("TLS-001", "CHAIN", Tone::Fail, 55.0),
            label("TLS-002", "OCSP", Tone::Fix, 65.0),
        ]),
        regions: vec![
            SpectrumRegion {
                label: "DNS".into(),
                x: 0.0,
                width: 50.0,
                grade: None,
            },
            SpectrumRegion {
                label: "TLS".into(),
                x: 50.0,
                width: 50.0,
                grade: None,
            },
        ],
    }
}

/// The element tag of the line with `data-code` inside the plate (the labels above it carry the
/// code too): a `<line>` or the `<g>` that groups a fixed one.
fn line_tag(html: &str, code: &str) -> String {
    let plate = html.find("<svg").unwrap();
    let needle = format!("data-code=\"{code}\"");
    let i = html[plate..]
        .find(&needle)
        .map(|i| i + plate)
        .unwrap_or_else(|| panic!("no {code}"));
    let start = html[..i].rfind('<').unwrap();
    let end = html[i..].find('>').unwrap() + i;
    html[start..end].to_string()
}

#[test]
fn spectrum_state_lines_take_their_width_token() {
    let html = spectrum(&fixed_view()).unwrap();
    let css = stylesheet("spectrum.css");
    for (code, tone, token) in [
        ("DNS-001", "pass", "--line-pass"),
        ("DNS-002", "warn", "--line-warn"),
        ("TLS-001", "fail", "--line-fail"),
    ] {
        let tag = line_tag(&html, code);
        assert!(tag.starts_with("<line "), "{code} is no <line>: {tag}");
        assert_eq!(attr(&tag, "data-tone"), Some(tone), "{tag}");
        assert!(
            attr(&tag, "style").is_none() && attr(&tag, "stroke-width").is_none(),
            "width must come from the stylesheet, not the markup: {tag}"
        );
        let sel = format!(".spectrum__line[data-tone=\"{tone}\"]");
        let width = resolve(&css, &[".spectrum__line", &sel], "stroke-width");
        assert_eq!(
            width.as_deref(),
            Some(format!("var({token})").as_str()),
            "{code} ({tone}) does not resolve to {token}"
        );
        assert!(TOKENS.contains(&format!("{token}:")), "{token} undefined");
    }
}

#[test]
fn spectrum_fixed_line_is_hollow_with_a_ring_on_top() {
    let html = spectrum(&fixed_view()).unwrap();
    let css = stylesheet("spectrum.css");
    let tag = line_tag(&html, "TLS-002");
    assert!(tag.starts_with("<g "), "fixed line must be a group: {tag}");
    assert_eq!(attr(&tag, "data-tone"), Some("fix"));

    let g_start = html.find(&tag).unwrap();
    let g_end = html[g_start..].find("</g>").unwrap() + g_start;
    let group = &html[g_start..g_end];
    assert!(
        !group.contains("<line "),
        "fixed line drawn as a bar: {group}"
    );
    let outline = group.find("<rect").expect("no outline shape");
    let ring = group.find("<circle").expect("no ring element");
    assert!(
        ring > outline,
        "ring must be painted after (on top of) the outline"
    );

    let outline_class = format!(".{}", attr(&group[outline..], "class").unwrap());
    let ring_class = format!(".{}", attr(&group[ring..], "class").unwrap());
    assert_eq!(
        resolve(&css, &[&outline_class], "fill").as_deref(),
        Some("none")
    );
    assert_eq!(
        resolve(&css, &[&outline_class], "width").as_deref(),
        Some("var(--line-fix)")
    );
    assert_eq!(
        resolve(&css, &[&outline_class], "stroke").as_deref(),
        Some("var(--status-fix-mark)")
    );
    assert_eq!(
        resolve(&css, &[&outline_class], "stroke-width").as_deref(),
        Some("var(--line-hair)"),
        "hollow outline is a hairline stroke"
    );
    let ring_stroke = resolve(&css, &[&ring_class], "stroke");
    assert_eq!(ring_stroke.as_deref(), Some("var(--status-fix-mark)"));
    // The group must not inherit a solid fill from the generic line rule.
    assert_ne!(
        resolve(&css, &[&outline_class], "fill").as_deref(),
        Some("var(--status-fix-mark)")
    );
}

#[test]
fn spectrum_unmeasured_line_uses_the_muted_mark() {
    let html = spectrum(&fixed_view()).unwrap();
    let css = stylesheet("spectrum.css");
    let tag = line_tag(&html, "TLS-003");
    assert!(tag.starts_with("<line "), "unmeasured line missing: {tag}");
    assert_eq!(attr(&tag, "data-tone"), Some("untested"));
    let stroke = resolve(
        &css,
        &[".spectrum__line", ".spectrum__line[data-tone=\"untested\"]"],
        "stroke",
    );
    assert_eq!(stroke.as_deref(), Some("var(--status-untested-mark)"));
    assert!(TOKENS.contains("--status-untested-mark:"));
    let width = resolve(
        &css,
        &[".spectrum__line", ".spectrum__line[data-tone=\"untested\"]"],
        "stroke-width",
    )
    .expect("unmeasured line has no width");
    assert!(
        width.starts_with("var(--line-"),
        "width {width} not a line token"
    );
}

#[test]
fn spectrum_one_hairline_separates_the_two_groups() {
    let html = spectrum(&fixed_view()).unwrap();
    let css = stylesheet("spectrum.css");
    let boundaries: Vec<&str> = tags(&html, "line")
        .into_iter()
        .filter(|t| attr(t, "class") == Some("spectrum__boundary"))
        .collect();
    let between: Vec<&&str> = boundaries
        .iter()
        .filter(|t| !matches!(attr(t, "x1"), Some("0%") | Some("100%")))
        .collect();
    assert_eq!(
        between.len(),
        1,
        "exactly one boundary between two groups: {boundaries:?}"
    );
    assert_eq!(attr(between[0], "x1"), Some("50.0000%"));
    assert_eq!(attr(between[0], "x1"), attr(between[0], "x2"), "vertical");
    assert_eq!(
        resolve(&css, &[".spectrum__boundary"], "stroke-width").as_deref(),
        Some("var(--line-hair)")
    );
}

#[test]
fn spectrum_group_names_below_and_short_names_above_the_plate() {
    let html = spectrum(&fixed_view()).unwrap();
    let plate_open = html.find("<svg").unwrap();
    let plate_close = html.find("</svg>").unwrap();
    let labels = html.find("spectrum__labels").unwrap();
    let axis = html.find("spectrum__axis").unwrap();
    assert!(labels < plate_open, "label row must precede the plate");
    assert!(axis > plate_close, "axis row must follow the plate");

    let above = &html[labels..plate_open];
    let plate = &html[plate_open..plate_close];
    let below = &html[axis..];
    for short in ["SPF", "CHAIN", "OCSP"] {
        assert!(above.contains(&format!(">{short}<")), "{short} not above");
        assert!(
            !plate.contains(short) && !below.contains(short),
            "{short} misplaced"
        );
    }
    assert_eq!(
        above.matches("spectrum__line-label").count(),
        3,
        "warn, fail and fixed only; none for pass or unmeasured: {above}"
    );
    assert!(!plate.contains("<text"), "no text inside the plate");
    for group in ["DNS", "TLS"] {
        assert!(
            below.contains(&format!(">{group}<")),
            "{group} not on the axis"
        );
        assert!(
            !above.contains(&format!(">{group}<")),
            "{group} above the plate"
        );
    }
}

#[test]
fn spectrum_pass_and_unmeasured_lines_carry_no_short_name_above_the_plate() {
    let html = spectrum(&SpectrumView {
        label: "Findings by position".into(),
        line_count: 3,
        rows: 1,
        bands: vec![band(
            "dns",
            None,
            vec![
                line("DNS-001", 5.0, Tone::Pass),
                line("DNS-002", 15.0, Tone::Untested),
                line("DNS-003", 25.0, Tone::Warn),
            ],
        )],
        label_sets: label_set(vec![label("DNS-003", "DKIM", Tone::Warn, 25.0)]),
        regions: vec![SpectrumRegion {
            label: "DNS".into(),
            x: 0.0,
            width: 100.0,
            grade: None,
        }],
    })
    .unwrap();
    let labels = html.find("spectrum__labels").unwrap();
    let plate_open = html.find("<svg").unwrap();
    let above = &html[labels..plate_open];
    assert!(above.contains(">DKIM<"), "warn short name missing: {above}");
    for short in ["MX", "SPF"] {
        assert!(
            !above.contains(&format!(">{short}<")),
            "{short} must not appear above the plate for pass or unmeasured: {above}"
        );
    }
    assert_eq!(
        above.matches("spectrum__line-label").count(),
        1,
        "only the warn line carries a short name: {above}"
    );
}

#[test]
fn trace_pass_row_is_a_hairline_and_fail_row_a_block() {
    let html = trace(&TraceView {
        rows: vec![
            TraceRow {
                date: "2026-10-15".into(),
                grade: "A".into(),
                cells: vec![Tone::Pass],
                current: true,
            },
            TraceRow {
                date: "2026-10-14".into(),
                grade: "D".into(),
                cells: vec![Tone::Fail],
                current: false,
            },
        ],
    })
    .unwrap();
    let css = stylesheet("trace.css");
    let cells: Vec<&str> = tags(&html, "span")
        .into_iter()
        .filter(|t| attr(t, "class") == Some("trace__cell"))
        .collect();
    assert_eq!(cells.len(), 2, "{html}");
    for cell in &cells {
        assert!(
            attr(cell, "style").is_none(),
            "height must come from the stylesheet: {cell}"
        );
    }
    let height = |tone: &str| {
        let sel = format!(".trace__cell[data-tone=\"{tone}\"]");
        resolve(&css, &[".trace__cell", &sel], "height")
    };
    assert_eq!(height("pass").as_deref(), Some("var(--line-hair)"));
    assert_eq!(height("fail").as_deref(), Some("var(--spectrum-trace-row)"));
    assert_eq!(
        resolve(
            &css,
            &[".trace__cell", ".trace__cell[data-tone=\"pass\"]"],
            "background"
        )
        .as_deref(),
        Some("var(--status-pass-hair)")
    );
    assert_eq!(
        resolve(
            &css,
            &[".trace__cell", ".trace__cell[data-tone=\"fail\"]"],
            "background"
        )
        .as_deref(),
        Some("var(--status-fail-mark)")
    );
}
