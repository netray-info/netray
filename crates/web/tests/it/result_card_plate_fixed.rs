//! The OG card's fixed line is the page's hollow outline (V2 spec v2-result-page, Phase 7,
//! requirement 29): a check that failed in the predecessor and passes now is drawn as a
//! hollow 5 px outline with its ring, not as a solid 2 px stroke.
use netray_engine::catalogue::catalogue;
use netray_model::run::{Run, RunCheck};
use netray_model::{Grade, Status};
use netray_web::plate::{card_layout, card_plate};

fn run_with_hsts(hsts: Status) -> Run {
    let checks = catalogue()
        .entries()
        .iter()
        .map(|e| RunCheck {
            id: e.id.clone(),
            status: if e.id.to_string() == "http.hsts" {
                hsts
            } else {
                Status::Pass
            },
            findings: Vec::new(),
            evidence: Vec::new(),
        })
        .collect();
    Run {
        profile_version: 3,
        engine_version: "0.0.0".into(),
        evidence_schema_version: 1,
        grade: Grade::B,
        complete: true,
        sections: Vec::new(),
        checks,
        blocks: Vec::new(),
    }
}

/// The element of `class="plate__line"` for `code`, from its opening tag to its end.
fn line_element<'a>(svg: &'a str, code: &str) -> &'a str {
    let needle = format!("class=\"plate__line\" data-code=\"{code}\"");
    let at = svg.find(&needle).expect("plate__line for the code");
    let start = svg[..at].rfind('<').unwrap();
    let rest = &svg[start..];
    if rest.starts_with("<g") {
        let end = rest.find("</g>").expect("group end") + "</g>".len();
        &rest[..end]
    } else {
        &rest[..rest.find('>').unwrap() + 1]
    }
}

#[test]
fn a_fixed_check_is_drawn_as_a_hollow_5px_outline_with_a_ring() {
    let pred = run_with_hsts(Status::Fail);
    let run = run_with_hsts(Status::Pass);
    let svg = card_plate(&card_layout(&run, Some(&pred)));

    let el = line_element(&svg, "http.hsts");
    assert!(el.starts_with("<g "), "a fixed line is a group: {el}");
    assert!(el.contains("data-tone=\"fixed\""), "{el}");
    let rect_at = el
        .find("<rect class=\"plate__outline\"")
        .expect("outline rect");
    let rect = &el[rect_at..rect_at + el[rect_at..].find('>').unwrap()];
    assert!(rect.contains("width=\"5\""), "{rect}");
    assert!(rect.contains("fill=\"none\""), "{rect}");
    assert!(rect.contains("stroke=\""), "{rect}");
    assert!(el.contains("class=\"plate__ring\""), "{el}");
    assert!(
        !el.contains("stroke-width=\"2\""),
        "no solid 2 px stroke: {el}"
    );

    // Every catalogue id is exactly one plate__line.
    assert_eq!(svg.matches("class=\"plate__line\"").count(), 54);
    for e in catalogue().entries() {
        let needle = format!("class=\"plate__line\" data-code=\"{}\"", e.id);
        assert_eq!(svg.matches(&needle).count(), 1, "{}", e.id);
    }
}
