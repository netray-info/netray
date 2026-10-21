//! v2-ip-ui Phase 3 (R14, C2, C4, C7): the IP card closes with the GeoLite2 attribution, rendered
//! from `netray_ip::GEOIP_ATTRIBUTION`; no template carries the text itself.
use std::path::Path;

use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_model::{CheckId, Status};
use netray_web::links::decorate;
use netray_web::pages::views::{ViewContext, render};

const FLAGS: [&str; 9] = [
    "tor",
    "c2",
    "spamhaus",
    "cins",
    "vpn",
    "datacenter",
    "anycast",
    "bot",
    "internal",
];

fn lookup_block() -> Block {
    let mut fields: Vec<(String, Value)> =
        vec![("address".into(), Value::Addr("192.0.2.10".parse().unwrap()))];
    for key in FLAGS {
        fields.push((key.into(), Value::Bool(true)));
    }
    Block {
        id: BlockId::parse("ip.lookup").unwrap(),
        title: "Lookup".into(),
        shape: Shape::Fields(fields),
    }
}

#[test]
fn c4_ip_card_ends_with_the_geoip_attribution_paragraph() {
    let statuses: Vec<(CheckId, Status)> = vec![];
    let decorate = decorate(&[]);
    let feeds = vec![];
    let ctx = ViewContext {
        statuses: &statuses,
        decorate: &decorate,
        feeds: &feeds,
    };
    let html = render(&lookup_block(), &ctx).expect("the view must claim an ip.lookup block");

    let constant = netray_ip::GEOIP_ATTRIBUTION;
    let (before, after) = constant
        .split_once("MaxMind")
        .expect("the constant names MaxMind");
    assert_eq!(before, "This product includes GeoLite2 data created by ");

    let expected = format!(
        "<p class=\"geoip-attribution\">{before}<a href=\"https://www.maxmind.com\" \
         rel=\"noopener noreferrer\">MaxMind</a>{after}</p>"
    );
    assert!(html.contains(&expected), "attribution missing in: {html}");

    // The card ends here: the explorer, when rendered, starts after it.
    let card_end = html
        .find("Use it from the command line")
        .unwrap_or(html.len());
    let start = html.find(&expected).unwrap();
    assert!(start < card_end, "attribution outside the card in: {html}");
    let tail = &html[start + expected.len()..card_end];
    for content in ["facts-grid", "ip-card__section"] {
        assert!(
            !tail.contains(content),
            "{content} follows the attribution in: {html}"
        );
    }
}

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            walk(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn c7_no_template_carries_the_attribution_text() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let mut files = vec![];
    walk(&root, &mut files);
    assert!(!files.is_empty(), "no templates found under {root:?}");
    for file in files {
        let Ok(content) = std::fs::read_to_string(&file) else {
            continue;
        };
        for needle in ["GeoLite2", "created by MaxMind"] {
            assert!(!content.contains(needle), "{file:?} contains {needle:?}");
        }
    }
}
