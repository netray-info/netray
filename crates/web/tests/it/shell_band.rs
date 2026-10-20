//! V2 spec v2-inspector-shell, requirement 8 (C1, C4, C5): after a run is applied to the page the
//! band sits under the h1 and before every evidence part; each line is named under the plate with
//! its catalogue short and line code; the caption states the zoom; the names have the board sizes.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, header};
use http_body_util::BodyExt;
use netray_engine::catalogue::catalogue;
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_model::{CheckResult, Protocol, Status};
use netray_web::pages::result::views::line_code;
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobPart, JobRequest, Tool, inspect_router,
};
use netray_web::tokens::CSS;
use tower::ServiceExt;

struct Stub;

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        let part = |n: usize| JobPart {
            label: format!("part{n}"),
            output: Box::pin(async move {
                Ok(vec![Block {
                    id: BlockId::parse(&format!("tls.chain{n}")).unwrap(),
                    title: "Chain".into(),
                    shape: Shape::Fields(vec![("subject".into(), Value::Text("leaf".into()))]),
                }])
            }),
        };
        let checks: Vec<CheckResult> = catalogue()
            .for_protocol(Protocol::Tls)
            .entries()
            .iter()
            .map(|s| CheckResult {
                id: s.id.clone(),
                status: Status::Pass,
                findings: Vec::new(),
                evidence: Vec::new(),
            })
            .collect();
        Ok(Job::new(vec![part(0), part(1)])
            .with_band(Protocol::Tls, Box::pin(async { Ok(checks) })))
    }
}

/// (fragment id, html) per fragment event of the run.
async fn run() -> Vec<(String, String)> {
    let app = inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()));
    let res = app
        .oneshot(
            Request::post("/inspect/tls/run")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("h=example.com"))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    let mut out = Vec::new();
    for raw in text.split("\n\n") {
        let mut name = "";
        let mut data = String::new();
        for line in raw.lines() {
            if let Some(v) = line.strip_prefix("event:") {
                name = v.trim();
            } else if let Some(v) = line.strip_prefix("data:") {
                data.push_str(v.trim_start());
            }
        }
        if name != "fragment" {
            continue;
        }
        let j: serde_json::Value = serde_json::from_str(&data).unwrap();
        out.push((
            j["id"].as_str().unwrap().to_owned(),
            j["html"].as_str().unwrap().to_owned(),
        ));
    }
    out
}

/// Replace the element carrying `id="<id>"` (open tag to its matching close) with `html`.
fn replace_element(page: &mut String, id: &str, html: &str) {
    let at = page
        .find(&format!(r#"id="{id}""#))
        .unwrap_or_else(|| panic!("no element {id} in {page}"));
    let open = page[..at].rfind('<').unwrap();
    let name: String = page[open + 1..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    let (o, c) = (format!("<{name}"), format!("</{name}>"));
    let (mut depth, mut i) = (0usize, open);
    while i < page.len() {
        let rest = &page[i..];
        if rest.starts_with(&o)
            && !rest[o.len()..].starts_with(|ch: char| ch.is_ascii_alphanumeric())
        {
            depth += 1;
        } else if rest.starts_with(&c) {
            depth -= 1;
            if depth == 0 {
                page.replace_range(open..i + c.len(), html);
                return;
            }
        }
        i += rest.chars().next().map_or(1, char::len_utf8);
    }
    panic!("unclosed element {id}");
}

async fn applied() -> String {
    let frags = run().await;
    assert_eq!(frags[0].0, "inspect-result", "{frags:?}");
    let mut result = frags[0].1.clone();
    for (id, html) in &frags[1..] {
        // Fragments outside the result region (the tabs) are not part of this page slice.
        if !result.contains(&format!(r#"id="{id}""#)) {
            continue;
        }
        replace_element(&mut result, id, html);
    }
    result
}

#[tokio::test]
async fn c1_c4_c5_band_is_under_the_h1_before_every_part_with_named_lines_and_caption() {
    let result = applied().await;

    // C4 / C1: h1, then the band, then every part.
    let h1 = result.find("<h1").expect("h1");
    let band = result
        .find(r#"id="inspect-band""#)
        .expect("inspect-band element");
    assert!(h1 < band, "{result}");
    for n in 0..2 {
        let part = result
            .find(&format!(r#"id="inspect-part-{n}""#))
            .unwrap_or_else(|| panic!("no inspect-part-{n}: {result}"));
        assert!(band < part, "band must precede part {n}: {result}");
    }
    assert!(
        result.contains(r#"<figure class="inspector-band">"#),
        "{result}"
    );

    // C5: the TLS band is 21 lines at positions 11-31 in today's catalogue.
    let tls = catalogue().for_protocol(Protocol::Tls);
    let specs = tls.entries();
    assert_eq!(specs.len(), 21);
    assert_eq!(line_code(&specs[0].id).as_deref(), Some("TLS\u{b7}11"));
    assert_eq!(line_code(&specs[20].id).as_deref(), Some("TLS\u{b7}31"));

    // Each line, in catalogue order, with its short and its code.
    let mut from = result
        .find(r#"class="inspector-band__lines""#)
        .expect("lines list");
    for spec in specs {
        let code = line_code(&spec.id).unwrap();
        let want = format!(
            r#"<span class="inspector-band__short">{}</span> <span class="inspector-band__code">{}</span>"#,
            spec.short, code
        );
        let at = result[from..]
            .find(&want)
            .unwrap_or_else(|| panic!("no `{want}` after offset {from}"));
        from += at + want.len();
    }

    assert!(
        result.contains(
            r#"<figcaption class="inspector-band__caption">The TLS band of the spectrum, zoomed: the same 21 lines as positions TLS·11–31 on the result page.</figcaption>"#
        ),
        "{result}"
    );
}

/// The last declaration of `prop` in the rule whose selector is exactly `selector`.
fn declared(selector: &str, prop: &str) -> Option<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/static");
    let mut found = None;
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        if e.path().extension().is_none_or(|x| x != "css") {
            continue;
        }
        let css = std::fs::read_to_string(e.path()).unwrap();
        let mut rest = css.as_str();
        while let Some(open) = rest.find('{') {
            let Some(close) = rest[open..].find('}') else {
                break;
            };
            if rest[..open].trim().rsplit('}').next().map(str::trim) == Some(selector) {
                for d in rest[open + 1..open + close].split(';') {
                    if let Some((k, v)) = d.split_once(':')
                        && k.trim() == prop
                    {
                        found = Some(v.trim().to_string());
                    }
                }
            }
            rest = &rest[open + close + 1..];
        }
    }
    found
}

fn resolve(value: &str) -> String {
    let Some(name) = value.strip_prefix("var(").and_then(|v| v.strip_suffix(')')) else {
        return value.to_string();
    };
    let needle = format!("{name}:");
    let at = CSS
        .find(&needle)
        .unwrap_or_else(|| panic!("unknown token {name}"));
    CSS[at + needle.len()..]
        .split(';')
        .next()
        .unwrap()
        .trim()
        .to_string()
}

#[test]
fn band_line_names_are_mono_at_11_and_10_px() {
    let size = |sel: &str| {
        resolve(&declared(sel, "font-size").unwrap_or_else(|| format!("<{sel} missing>")))
    };
    assert_eq!(size(".inspector-band__short"), "11px");
    assert_eq!(size(".inspector-band__code"), "10px");
    for sel in [".inspector-band__short", ".inspector-band__code"] {
        let family = declared(sel, "font-family").unwrap_or_default();
        assert!(
            family.contains("mono"),
            "{sel} must use the mono family: {family:?}"
        );
    }
}
