//! The spectrum component renders a layout (V2 spec v2-spectrum, Phase 3, requirements 15 and
//! 18): label rows, leaders, scroll box, bands, per-run groups and label sets.
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use netray_engine::catalogue;
use netray_engine::profile::{Profile, Profiles};
use netray_model::{CheckId, Protocol, Status};
use netray_web::components::{SpectrumView, spectrum};
use netray_web::spectrum::{Layout, RunInput, SectionState, layout};

/// The plate width the page places at: 57 checks at the narrowest pitch.
const W: u32 = 456;

fn id(s: &str) -> CheckId {
    CheckId::parse(s).expect("check id")
}

fn statuses(over: &[(CheckId, Option<Status>)]) -> BTreeMap<CheckId, Status> {
    let mut m: BTreeMap<CheckId, Status> = catalogue()
        .entries()
        .iter()
        .map(|e| (e.id.clone(), Status::Pass))
        .collect();
    for (k, v) in over {
        match v {
            Some(s) => {
                m.insert(k.clone(), *s);
            }
            None => {
                m.remove(k);
            }
        }
    }
    m
}

fn run_with<'a>(
    statuses: BTreeMap<CheckId, Status>,
    sections: &[(Protocol, SectionState)],
    profile: &'a Profile,
) -> RunInput<'a> {
    RunInput {
        statuses,
        sections: sections.iter().copied().collect(),
        profile,
    }
}

/// A run with every check passing except `over`.
fn run(over: &[(CheckId, Option<Status>)]) -> RunInput<'static> {
    run_with(statuses(over), &[], Profiles::current())
}

/// The first `n` catalogue checks of `protocol`, each failing.
fn failing(protocol: Protocol, n: usize) -> Vec<(CheckId, Option<Status>)> {
    catalogue()
        .entries()
        .iter()
        .filter(|e| e.protocol == protocol)
        .take(n)
        .map(|e| (e.id.clone(), Some(Status::Fail)))
        .collect()
}

fn lay(current: &RunInput, older: &[RunInput]) -> Layout {
    layout(current, older, catalogue(), W).expect("layout")
}

fn render(l: &Layout, shown: usize) -> String {
    let view = SpectrumView::from_layout(l, "Checks by protocol", shown);
    spectrum(&view).expect("render")
}

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

fn has_class(tag: &str, class: &str) -> bool {
    attr(tag, "class") == Some(class)
}

/// The text of band `protocol`'s group: from its opening tag to the next band or the plate's end.
fn band<'a>(html: &'a str, protocol: &str) -> &'a str {
    let open = format!("<g id=\"band-{protocol}\"");
    let start = html
        .find(&open)
        .unwrap_or_else(|| panic!("no band-{protocol}"));
    let rest = &html[start + open.len()..];
    let end = [rest.find("<g id=\"band-"), rest.find("</svg>")]
        .into_iter()
        .flatten()
        .min()
        .expect("unterminated band");
    &rest[..end]
}

/// The run groups of a band: (opening tag, content), in document order.
fn run_groups(band: &str) -> Vec<(String, &str)> {
    let marker = "<g class=\"spectrum__run\"";
    band.split(marker)
        .skip(1)
        .map(|chunk| {
            let end = chunk.find('>').expect("unterminated run group");
            (format!("{marker}{}", &chunk[..end]), &chunk[end..])
        })
        .collect()
}

/// The `spectrum__line` marks (plain `<line` ones) of a run group.
fn line_marks(group: &str) -> Vec<&str> {
    tags(group, "line")
        .into_iter()
        .filter(|t| has_class(t, "spectrum__line"))
        .collect()
}

/// The label sets: (opening tag, content up to its close), in document order.
fn label_sets(html: &str) -> Vec<(String, &str)> {
    let marker = "<div class=\"spectrum__labels\"";
    html.split(marker)
        .skip(1)
        .map(|chunk| {
            let end = chunk.find('>').expect("unterminated label set");
            let body = &chunk[end..];
            let close = body.find("</div>").expect("unclosed label set");
            (format!("{marker}{}", &chunk[..end]), &body[..close])
        })
        .collect()
}

fn labels_of(set: &str) -> Vec<&str> {
    tags(set, "span")
        .into_iter()
        .filter(|t| has_class(t, "spectrum__line-label"))
        .collect()
}

fn leaders_of(set: &str) -> Vec<&str> {
    tags(set, "span")
        .into_iter()
        .filter(|t| has_class(t, "spectrum__leader"))
        .collect()
}

fn calc_rows(row: usize) -> String {
    format!("calc(var(--spectrum-label-leader) + {row} * var(--spectrum-label-row))")
}

fn band_height(html: &str) -> String {
    let t = tags(html, "div")
        .into_iter()
        .find(|t| has_class(t, "spectrum__label-band"))
        .expect("label band");
    let style = attr(t, "style").expect("label band style");
    style
        .strip_prefix("height: ")
        .expect("height first")
        .trim_end_matches(';')
        .to_string()
}

fn style_value<'a>(tag: &'a str, prop: &str) -> &'a str {
    let style = attr(tag, "style").unwrap_or_else(|| panic!("no style in {tag}"));
    let needle = format!("{prop}: ");
    let i = style
        .find(&needle)
        .unwrap_or_else(|| panic!("no {prop} in {style}"))
        + needle.len();
    // A value may hold `;`-free parentheses only; it ends at the next `;` or the end.
    let rest = &style[i..];
    rest[..rest.find(';').unwrap_or(rest.len())].trim()
}

fn css_rule_body(css: &str, selector: &str) -> String {
    let mut rest = css;
    let mut body = String::new();
    while let Some(open) = rest.find('{') {
        let close = rest[open..].find('}').expect("unclosed rule") + open;
        if rest[..open].split(',').any(|s| s.trim() == selector) {
            body.push_str(&rest[open + 1..close]);
            body.push(';');
        }
        rest = &rest[close + 1..];
    }
    body
}

fn tone_of(group: &str, code: &str) -> String {
    let needle = format!("data-code=\"{code}\"");
    let i = group.find(&needle).expect("code in group");
    let start = group[..i].rfind('<').expect("tag start");
    let end = group[i..].find('>').unwrap() + i;
    attr(&group[start..end], "data-tone")
        .expect("data-tone")
        .to_string()
}

#[test]
fn c1_label_edge_shift_and_plate_height() {
    // The last line (`ip.reputation`, x = 452 px of 456) fails: its box is shifted off the edge.
    let l = lay(&run(&[(id("ip.reputation"), Some(Status::Fail))]), &[]);
    let html = render(&l, 0);

    let set = &label_sets(&html)[0];
    let labels = labels_of(set.1);
    assert_eq!(labels.len(), 1);
    let width = format!("{:.2}", l.runs[0].labels[0].width);
    assert_eq!(
        style_value(labels[0], "left"),
        format!("clamp(0px, 99.1228%, calc(100% - {width}px))")
    );
    let leaders = leaders_of(set.1);
    assert_eq!(style_value(leaders[0], "left"), "99.1228%");

    // The plate height: y2 is 100 minus the layout height in percent.
    let l = lay(&run(&[(id("dns.dnssec"), Some(Status::Fail))]), &[]);
    let dnssec = l
        .lines
        .iter()
        .position(|x| x.id == id("dns.dnssec"))
        .unwrap();
    let h = l.runs[0].heights[dnssec];
    let html = render(&l, 0);
    let band = band(&html, "dns");
    let marks = line_marks(band);
    assert_eq!(attr(marks[0], "y1"), Some("100%"));
    let expected = format!("{:.4}%", 100.0 - h * 100.0);
    assert_eq!(attr(marks[0], "y2"), Some(expected.as_str()));
}

#[test]
fn c4_bands_runs_and_label_sets_exist_per_protocol() {
    let l = lay(&run(&[]), &[run(&[]), run(&[])]);
    let html = render(&l, 0);
    for p in ["dns", "tls", "http", "email", "ip"] {
        assert!(
            html.contains(&format!("<g id=\"band-{p}\" data-protocol=\"{p}\"")),
            "band-{p}"
        );
        assert_eq!(run_groups(band(&html, p)).len(), 3);
    }
    assert_eq!(label_sets(&html).len(), 3);
}

#[test]
fn c5_first_line_x_and_scroll_box_min_width() {
    let l = lay(&run(&[(id("dns.dnssec"), Some(Status::Fail))]), &[]);
    let html = render(&l, 0);
    let marks = line_marks(band(&html, "dns"));
    assert_eq!(attr(marks[0], "x1"), Some("0.8772%"));
    assert_eq!(attr(marks[0], "x2"), Some("0.8772%"));
    let boxed = tags(&html, "div")
        .into_iter()
        .find(|t| has_class(t, "spectrum__box"))
        .expect("box");
    assert_eq!(attr(boxed, "style"), Some("--spectrum-lines: 57"));
}

#[test]
fn c6_row_two_label_sits_on_the_calc_and_its_leader_is_36px() {
    let l = lay(&run(&failing(Protocol::Dns, 3)), &[]);
    let html = render(&l, 0);
    let set = &label_sets(&html)[0];
    let label = labels_of(set.1)
        .into_iter()
        .find(|t| style_value(t, "bottom") == calc_rows(2))
        .expect("a label in row 2");
    let tone = attr(label, "data-tone").unwrap();
    let leader = leaders_of(set.1)
        .into_iter()
        .find(|t| {
            style_value(t, "height")
                == "calc(var(--spectrum-label-leader) + 2 * var(--spectrum-label-row))"
        })
        .expect("a leader of row 2");
    assert_eq!(attr(leader, "data-tone"), Some(tone));
    // The third dns line sits at x = 20 of 456 px.
    assert_eq!(style_value(leader, "left"), "4.3860%");
}

#[test]
fn c7_band_height_follows_the_largest_row_count() {
    let four = lay(&run(&failing(Protocol::Dns, 4)), &[]);
    assert_eq!(four.runs[0].rows, 4);
    assert_eq!(
        band_height(&render(&four, 0)),
        "calc(var(--spectrum-label-band) + 1 * var(--spectrum-label-row))"
    );

    let one = lay(&run(&failing(Protocol::Dns, 1)), &[]);
    assert_eq!(
        band_height(&render(&one, 0)),
        "calc(var(--spectrum-label-band) + 0 * var(--spectrum-label-row))"
    );

    // The current run fails dns 1-3 and has dns 4-5 not applicable, so the older run's fails on
    // dns 4-5 are not fixed in it (a fixed line would carry a label and a row of its own).
    let mut current = failing(Protocol::Dns, 5);
    for entry in &mut current[3..] {
        entry.1 = Some(Status::NotApplicable);
    }
    let mixed = lay(&run(&current), &[run(&failing(Protocol::Dns, 5))]);
    assert_eq!(mixed.runs[0].rows, 3);
    assert_eq!(mixed.runs[1].rows, 5);
    assert_eq!(
        band_height(&render(&mixed, 0)),
        "calc(var(--spectrum-label-band) + 2 * var(--spectrum-label-row))"
    );
}

#[test]
fn c8_bands_in_protocol_order_with_their_lines() {
    let html = render(&lay(&run(&[]), &[run(&[])]), 0);
    let order = ["dns", "tls", "http", "email", "ip"];
    let counts = [10, 23, 13, 10, 1];
    let mut last = 0;
    for (p, n) in order.iter().zip(counts) {
        let needle = format!("id=\"band-{p}\"");
        assert_eq!(html.matches(&needle).count(), 1, "{needle} once");
        let at = html.find(&needle).unwrap();
        assert!(at >= last, "band-{p} out of order");
        last = at;
        let groups = run_groups(band(&html, p));
        assert_eq!(groups.len(), 2);
        for (_, g) in groups {
            assert_eq!(g.matches("class=\"spectrum__line\"").count(), n, "{p}");
        }
    }
}

#[test]
fn c9_only_the_shown_run_carries_data_shown() {
    let l = lay(&run(&[]), &[run(&[]), run(&[])]);
    for shown in [0usize, 1] {
        let html = render(&l, shown);
        for p in ["dns", "tls", "http", "email", "ip"] {
            let groups = run_groups(band(&html, p));
            assert_eq!(groups.len(), 3);
            for (k, (open, _)) in groups.iter().enumerate() {
                assert_eq!(attr(open, "data-run"), Some(k.to_string().as_str()));
                assert_eq!(open.contains(" data-shown"), k == shown, "{p} run {k}");
            }
        }
        let sets = label_sets(&html);
        assert_eq!(sets.len(), 3);
        for (k, (open, _)) in sets.iter().enumerate() {
            assert_eq!(attr(open, "data-run"), Some(k.to_string().as_str()));
            assert_eq!(open.contains(" data-shown"), k == shown, "labels run {k}");
        }
    }
}

#[test]
fn c10_unmeasured_section_marks_only_its_band() {
    let cur = run_with(
        statuses(&[]),
        &[(Protocol::Email, SectionState::Unmeasured)],
        Profiles::current(),
    );
    let html = render(&lay(&cur, &[]), 0);
    for p in ["dns", "tls", "http", "email", "ip"] {
        let groups = run_groups(band(&html, p));
        assert_eq!(
            groups[0].0.contains(" data-unmeasured"),
            p == "email",
            "{p}"
        );
    }
    let email = run_groups(band(&html, "email"));
    let marks = line_marks(email[0].1);
    assert_eq!(marks.len(), 10);
    for m in marks {
        assert_eq!(attr(m, "data-tone"), Some("untested"));
        let y2: f64 = attr(m, "y2")
            .unwrap()
            .trim_end_matches('%')
            .parse()
            .unwrap();
        assert!(y2 < 100.0, "muted line keeps a height: {m}");
    }
}

#[test]
fn c11_not_applicable_section_is_not_unmeasured() {
    let cur = run_with(
        statuses(&[]),
        &[
            (Protocol::Tls, SectionState::NotApplicable),
            (Protocol::Http, SectionState::NotApplicable),
        ],
        Profiles::current(),
    );
    let html = render(&lay(&cur, &[]), 0);
    assert!(!html.contains("data-unmeasured"));
    for p in ["tls", "http"] {
        let groups = run_groups(band(&html, p));
        for m in line_marks(groups[0].1) {
            assert_eq!(attr(m, "data-tone"), Some("untested"), "{p}");
            let y2: f64 = attr(m, "y2")
                .unwrap()
                .trim_end_matches('%')
                .parse()
                .unwrap();
            assert!(y2 < 100.0, "muted line keeps a height: {m}");
        }
    }
}

#[test]
fn c12_runs_differ_only_in_data_shown() {
    let l = lay(
        &run(&failing(Protocol::Dns, 2)),
        &[run(&failing(Protocol::Tls, 2))],
    );
    let a = render(&l, 0);
    let b = render(&l, 1);
    assert_ne!(a, b);
    assert_eq!(a.replace(" data-shown", ""), b.replace(" data-shown", ""));
}

#[test]
fn c13_leaders_precede_transparent_labels() {
    let l = lay(&run(&failing(Protocol::Dns, 2)), &[]);
    assert_eq!(l.runs[0].labels.len(), 2);
    let html = render(&l, 0);
    for (_, set) in label_sets(&html) {
        let last_leader = set.rfind("class=\"spectrum__leader\"").expect("a leader");
        let first_label = set.find("class=\"spectrum__line-label\"").expect("a label");
        assert!(last_leader < first_label, "a leader follows a label");
        assert_eq!(set.matches("class=\"spectrum__leader\"").count(), 2);
        assert_eq!(set.matches("class=\"spectrum__line-label\"").count(), 2);
    }
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static/spectrum.css");
    let css = fs::read_to_string(&path).expect("spectrum.css");
    let body = css_rule_body(&css, ".spectrum__line-label");
    assert!(
        body.split(';')
            .any(|d| d.trim().replace(' ', "") == "background:transparent"),
        "no transparent background in {body}"
    );
}

#[test]
fn c14_pass_and_muted_lines_have_no_label_or_leader() {
    // `dns.dnssec` fails; `dns.caa` is absent (muted); every other check passes.
    let l = lay(
        &run(&[
            (id("dns.dnssec"), Some(Status::Fail)),
            (id("dns.caa"), None),
        ]),
        &[],
    );
    let html = render(&l, 0);
    let dns = band(&html, "dns");
    let group = run_groups(dns)[0].1;
    let code_of = |tone: &str| -> Vec<String> {
        line_marks(group)
            .into_iter()
            .filter(|t| attr(t, "data-tone") == Some(tone))
            .map(|t| attr(t, "data-code").unwrap().to_string())
            .collect()
    };
    let fail = code_of("fail");
    assert_eq!(fail.len(), 1);
    let muted = code_of("untested");
    let pass = code_of("pass");
    assert!(!muted.is_empty() && !pass.is_empty());
    assert_eq!(tone_of(group, &fail[0]), "fail");

    let set = &label_sets(&html)[0];
    let labels = labels_of(set.1);
    let leaders = leaders_of(set.1);
    assert_eq!(labels.len(), 1);
    assert_eq!(leaders.len(), 1);
    assert_eq!(attr(labels[0], "data-code"), Some(fail[0].as_str()));
    for c in muted.iter().chain(&pass) {
        assert!(!set.1.contains(&format!("data-code=\"{c}\"")), "{c}");
    }
}
