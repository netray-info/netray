//! Contract tests for the ten design-system components (V2 spec v2-design-system, Phase 3, C2):
//! a fixed view per component renders markup that carries the root class and the view's values.
use netray_web::components::{
    CheckRowView, DiffBlockView, DiffEntry, EvidenceBlockView, EvidenceShape, Field, GradeView,
    GroupingOption, GroupingSwitchView, ProgressRayView, QueryInputView, RaySegment, RecordRow,
    RunLineView, SegmentState, SpectrumBand, SpectrumBandRun, SpectrumLabel, SpectrumLabelSet,
    SpectrumLine, SpectrumRegion, SpectrumView, Tone, TraceRow, TraceView, TreeItem, check_row,
    diff_block, evidence_block, grade, grouping_switch, progress_ray, query_input, run_line,
    spectrum, trace,
};

/// Every class token found in any `class="..."` attribute of the markup.
fn class_tokens(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find("class=\"") {
        rest = &rest[i + 7..];
        let Some(end) = rest.find('"') else { break };
        out.extend(rest[..end].split_whitespace().map(str::to_string));
        rest = &rest[end..];
    }
    out
}

fn assert_root(html: &str, root: &str) {
    assert!(
        class_tokens(html).iter().any(|t| t == root),
        "no class token `{root}` in: {html}"
    );
}

fn assert_has(html: &str, needle: &str) {
    assert!(html.contains(needle), "missing `{needle}` in: {html}");
}

fn assert_lacks(html: &str, needle: &str) {
    assert!(!html.contains(needle), "unexpected `{needle}` in: {html}");
}

fn assert_all(html: &str, needles: &[&str]) {
    for n in needles {
        assert_has(html, n);
    }
}

#[test]
fn grade_renders_letter_score_and_tone() {
    let html = grade(&GradeView {
        letter: "B".into(),
        score: Some(87),
        tone: Tone::Warn,
    })
    .unwrap();
    assert_root(&html, "grade");
    assert_all(&html, &["data-tone=\"warn\"", "B", "87", "/ 100"]);

    let bare = grade(&GradeView {
        letter: "F".into(),
        score: None,
        tone: Tone::Fail,
    })
    .unwrap();
    assert_root(&bare, "grade");
    assert_all(&bare, &["data-tone=\"fail\"", "F"]);
    assert_lacks(&bare, "/ 100");
}

#[test]
fn run_line_renders_parts_in_order() {
    let html = run_line(&RunLineView {
        parts: vec!["example.com".into(), "run 4711".into(), "2026-10-15".into()],
    })
    .unwrap();
    assert_root(&html, "run-line");
    let at: Vec<usize> = ["example.com", "run 4711", "2026-10-15"]
        .iter()
        .map(|p| {
            html.find(p)
                .unwrap_or_else(|| panic!("missing `{p}` in: {html}"))
        })
        .collect();
    assert!(
        at[0] < at[1] && at[1] < at[2],
        "parts out of order in: {html}"
    );
}

#[test]
fn progress_ray_renders_label_and_segment_states() {
    let html = progress_ray(&ProgressRayView {
        label: "Scan progress".into(),
        segments: vec![
            RaySegment {
                label: "DNS".into(),
                state: SegmentState::Done,
            },
            RaySegment {
                label: "TLS".into(),
                state: SegmentState::Pending,
            },
            RaySegment {
                label: "HTTP".into(),
                state: SegmentState::Incomplete,
            },
        ],
    })
    .unwrap();
    assert_root(&html, "progress-ray");
    assert_all(
        &html,
        &[
            "aria-label=\"Scan progress\"",
            "data-state=\"done\"",
            "data-state=\"pending\"",
            "data-state=\"incomplete\"",
            "DNS",
            "TLS",
            "HTTP",
        ],
    );
}

#[test]
fn spectrum_renders_plate_lines_and_regions() {
    let html = spectrum(&SpectrumView {
        label: "Findings by position".into(),
        line_count: 2,
        rows: 1,
        bands: vec![
            SpectrumBand {
                protocol: "dns".into(),
                boundary: None,
                runs: vec![SpectrumBandRun {
                    run: 0,
                    shown: true,
                    unmeasured: false,
                    lines: vec![SpectrumLine {
                        code: "DNS-017".into(),
                        x: 12.5,
                        height: 40.0,
                        tone: Tone::Fail,
                    }],
                }],
            },
            SpectrumBand {
                protocol: "tls".into(),
                boundary: Some(50.0),
                runs: vec![SpectrumBandRun {
                    run: 0,
                    shown: true,
                    unmeasured: false,
                    lines: vec![SpectrumLine {
                        code: "TLS-003".into(),
                        x: 60.0,
                        height: 20.0,
                        tone: Tone::Pass,
                    }],
                }],
            },
        ],
        label_sets: vec![SpectrumLabelSet {
            run: 0,
            shown: true,
            labels: vec![SpectrumLabel {
                code: "DNS-017".into(),
                text: "Dangling CNAME".into(),
                tone: Tone::Fail,
                row: 0,
                x: 12.5,
                width: 100.0,
            }],
        }],
        regions: vec![SpectrumRegion {
            label: "Transport zone".into(),
            x: 50.0,
            width: 25.0,
        }],
    })
    .unwrap();
    assert_root(&html, "spectrum");
    assert_all(
        &html,
        &[
            "<svg",
            "role=\"img\"",
            "aria-label=\"Findings by position\"",
            "data-code=\"DNS-017\"",
            "data-code=\"TLS-003\"",
            "data-tone=\"fail\"",
            "data-tone=\"pass\"",
            "Dangling CNAME",
            "Transport zone",
        ],
    );
}

#[test]
fn trace_renders_rows_cells_and_single_current_marker() {
    let html = trace(&TraceView {
        rows: vec![
            TraceRow {
                date: "2026-10-15".into(),
                grade: "A-".into(),
                cells: vec![Tone::Pass, Tone::Warn, Tone::Fail],
                current: true,
            },
            TraceRow {
                date: "2026-09-01".into(),
                grade: "C+".into(),
                cells: vec![Tone::Fix, Tone::Untested],
                current: false,
            },
        ],
    })
    .unwrap();
    assert_root(&html, "trace");
    assert_all(&html, &["2026-10-15", "A-", "2026-09-01", "C+"]);
    assert_eq!(
        html.matches("aria-current=\"true\"").count(),
        1,
        "in: {html}"
    );
    assert_eq!(html.matches("aria-current").count(), 1, "in: {html}");
    assert!(
        class_tokens(&html).iter().any(|t| t == "marker"),
        "no marker in: {html}"
    );
    for tone in ["pass", "warn", "fail", "fix", "untested"] {
        assert_eq!(
            html.matches(&format!("data-tone=\"{tone}\"")).count(),
            1,
            "cell count for {tone} in: {html}"
        );
    }
    assert_eq!(
        html.matches("data-tone=").count(),
        5,
        "cell count in: {html}"
    );
}

#[test]
fn check_row_renders_values_links_and_escapes() {
    let full = CheckRowView {
        code: "HTML-042".into(),
        tone: Tone::Fix,
        title: "Missing <b> & tag".into(),
        since: Some("since v0.9".into()),
        body: "The page lacks a canonical link.".into(),
        fix: Some("Add a canonical link element.".into()),
        evidence_href: Some("/check/example.com#ev-1".into()),
        guide_href: Some("https://netray.info/guide/html-042".into()),
    };
    let html = check_row(&full).unwrap();
    assert_root(&html, "check-row");
    assert_all(
        &html,
        &[
            "data-tone=\"fix\"",
            "HTML-042",
            "since v0.9",
            "The page lacks a canonical link.",
            "Fix:",
            "Add a canonical link element.",
            "href=\"/check/example.com#ev-1\"",
            "href=\"https://netray.info/guide/html-042\"",
            "Missing &#60;b&#62; &#38; tag",
        ],
    );
    assert_lacks(&html, "<b>");

    let bare = check_row(&CheckRowView {
        since: None,
        fix: None,
        evidence_href: None,
        guide_href: None,
        ..full
    })
    .unwrap();
    assert_root(&bare, "check-row");
    assert_lacks(&bare, "Fix:");
    assert_lacks(&bare, "since v0.9");
    assert_lacks(&bare, "href=");
}

fn evidence(id: &str, shape: EvidenceShape) -> String {
    evidence_block(&EvidenceBlockView {
        id: id.into(),
        title: format!("Evidence {id}"),
        feeds: vec!["DNS-017".into(), "TLS-003".into()],
        shape,
    })
    .unwrap()
}

fn field(key: &str, value: &str) -> Field {
    Field {
        key: key.into(),
        value: value.into(),
    }
}

#[test]
fn evidence_block_renders_every_shape() {
    let cases: Vec<(EvidenceShape, &str, Vec<&str>)> = vec![
        (
            EvidenceShape::Records(vec![RecordRow {
                owner: "www.example.com".into(),
                rtype: "CNAME".into(),
                ttl: 3600,
                data: "edge.example.com".into(),
            }]),
            "records",
            vec!["www.example.com", "CNAME", "3600", "edge.example.com"],
        ),
        (
            EvidenceShape::Table {
                columns: vec!["Resolver".into(), "Latency".into()],
                rows: vec![vec!["ns1.example.com".into(), "23 ms".into()]],
            },
            "table",
            vec!["Resolver", "Latency", "ns1.example.com", "23 ms"],
        ),
        (
            EvidenceShape::Chain(vec![
                vec![
                    field("Subject", "leaf.example.com"),
                    field("Issuer", "Example CA"),
                ],
                vec![
                    field("Subject", "Example CA"),
                    field("Issuer", "Example Root"),
                ],
            ]),
            "chain",
            vec![
                "Subject",
                "leaf.example.com",
                "Issuer",
                "Example CA",
                "Example Root",
            ],
        ),
        (
            EvidenceShape::Tree(vec![
                TreeItem {
                    depth: 0,
                    text: "example.com".into(),
                },
                TreeItem {
                    depth: 2,
                    text: "mail.example.com".into(),
                },
            ]),
            "tree",
            vec![
                "data-depth=\"0\"",
                "data-depth=\"2\"",
                "example.com",
                "mail.example.com",
            ],
        ),
        (
            EvidenceShape::Fields(vec![
                field("Protocol", "TLSv1.3"),
                field("Cipher", "AES-256"),
            ]),
            "fields",
            vec!["Protocol", "TLSv1.3", "Cipher", "AES-256"],
        ),
        (
            EvidenceShape::Text("HTTP/1.1 200 OK\nServer: example".into()),
            "text",
            vec!["<pre", "HTTP/1.1 200 OK\nServer: example"],
        ),
    ];
    for (i, (shape, name, needles)) in cases.into_iter().enumerate() {
        let id = format!("ev-{i}");
        let html = evidence(&id, shape);
        assert_root(&html, "evidence-block");
        assert_all(
            &html,
            &[
                &format!("id=\"{id}\""),
                &format!("data-shape=\"{name}\""),
                &format!("Evidence {id}"),
                "DNS-017",
                "TLS-003",
            ],
        );
        assert_all(&html, needles.as_slice());
    }
}

#[test]
fn diff_block_renders_entries_and_note() {
    let entries = vec![
        DiffEntry {
            kind: "Added".into(),
            tone: Tone::Pass,
            text: "AAAA record published".into(),
            protocol: "IPv6".into(),
        },
        DiffEntry {
            kind: "Removed".into(),
            tone: Tone::Fail,
            text: "TLS 1.0 disabled".into(),
            protocol: "TLS".into(),
        },
    ];
    let html = diff_block(&DiffBlockView {
        title: "Changes since last run".into(),
        summary: "2 changes".into(),
        entries,
        note: None,
    })
    .unwrap();
    assert_root(&html, "diff-block");
    assert_all(
        &html,
        &[
            "Changes since last run",
            "2 changes",
            "Added",
            "AAAA record published",
            "IPv6",
            "data-tone=\"pass\"",
            "Removed",
            "TLS 1.0 disabled",
            "data-tone=\"fail\"",
        ],
    );

    let empty = diff_block(&DiffBlockView {
        title: "Changes since last run".into(),
        summary: "no changes".into(),
        entries: vec![],
        note: Some("Nothing to compare yet.".into()),
    })
    .unwrap();
    assert_root(&empty, "diff-block");
    assert_has(&empty, "Nothing to compare yet.");
}

#[test]
fn grouping_switch_renders_options_with_pressed_state() {
    let html = grouping_switch(&GroupingSwitchView {
        label: "Group findings".into(),
        options: vec![
            GroupingOption {
                value: "severity".into(),
                label: "By severity".into(),
                pressed: true,
            },
            GroupingOption {
                value: "layer".into(),
                label: "By layer".into(),
                pressed: false,
            },
        ],
    })
    .unwrap();
    assert_root(&html, "grouping-switch");
    assert_all(
        &html,
        &[
            "role=\"group\"",
            "aria-label=\"Group findings\"",
            "data-group=\"severity\"",
            "data-group=\"layer\"",
            "By severity",
            "By layer",
        ],
    );
    assert_eq!(
        html.matches("<button type=\"button\"").count(),
        2,
        "in: {html}"
    );
    assert_eq!(
        html.matches("aria-pressed=\"true\"").count(),
        1,
        "in: {html}"
    );
    assert_eq!(
        html.matches("aria-pressed=\"false\"").count(),
        1,
        "in: {html}"
    );
    assert!(
        class_tokens(&html).iter().any(|t| t == "marker"),
        "no marker in: {html}"
    );
}

#[test]
fn query_input_renders_form_fields() {
    let view = QueryInputView {
        action: "/check".into(),
        name: "target".into(),
        label: "Domain to check".into(),
        placeholder: "example.com".into(),
        value: Some("netray.info".into()),
        submit: "Run check".into(),
    };
    let html = query_input(&view).unwrap();
    assert_root(&html, "query-input");
    assert_all(
        &html,
        &[
            "<form class=\"query-input",
            "action=\"/check\"",
            "method=\"get\"",
            "<label",
            "Domain to check",
            "<input",
            "name=\"target\"",
            "placeholder=\"example.com\"",
            "value=\"netray.info\"",
            "Run check",
        ],
    );

    let empty = query_input(&QueryInputView {
        value: None,
        ..view
    })
    .unwrap();
    assert_root(&empty, "query-input");
    assert_lacks(&empty, "value=\"netray.info\"");
}
