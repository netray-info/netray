//! v2-ip-ui Phase 2 (R9, R10; C2, C3, C9, C10, C11): the IP card's "Location" facts_grid
//! section with human labels, the coordinates cell and the OpenStreetMap link.
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

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

/// Render an `ip.lookup` block of the address, the nine false flags and `extra`;
/// entity escapes of `&` are normalised to a raw `&`.
fn html_of(extra: Vec<(&str, Value)>) -> String {
    let mut fields: Vec<(String, Value)> =
        vec![("address".into(), Value::Addr("192.0.2.10".parse().unwrap()))];
    for flag in FLAGS {
        fields.push((flag.into(), Value::Bool(false)));
    }
    for (k, v) in extra {
        fields.push((k.into(), v));
    }
    let block = Block {
        id: BlockId::parse("ip.lookup").unwrap(),
        title: "Lookup".into(),
        shape: Shape::Fields(fields),
    };
    let statuses: Vec<(CheckId, Status)> = vec![];
    let decorate = decorate(&[]);
    let feeds = vec![];
    let ctx = ViewContext {
        statuses: &statuses,
        decorate: &decorate,
        feeds: &feeds,
    };
    let html = render(&block, &ctx).expect("the view must claim the ip.lookup block");
    html.replace("&#38;", "&").replace("&amp;", "&")
}

fn label(name: &str) -> String {
    format!("<span class=\"facts-grid__label\">{name}</span>")
}

/// The markup following the label up to the next label: the label's cell.
fn cell<'a>(html: &'a str, name: &str) -> &'a str {
    let l = label(name);
    let start = html
        .find(&l)
        .unwrap_or_else(|| panic!("label {name:?} missing in: {html}"))
        + l.len();
    let rest = &html[start..];
    let end = rest.find("facts-grid__label").unwrap_or(rest.len());
    &rest[..end]
}

const MAP_HREF: &str =
    "href=\"https://www.openstreetmap.org/?mlat=52.52&mlon=13.4#map=10/52.52/13.4\"";

#[test]
fn c2_location_section_has_human_labels() {
    let html = html_of(vec![
        ("city", text("Berlin")),
        ("region", text("Berlin")),
        ("region_code", text("BE")),
        ("country", text("Germany")),
        ("country_iso", text("DE")),
        ("continent", text("Europe")),
        ("postal_code", text("10115")),
        ("timezone", text("Europe/Berlin")),
    ]);
    assert!(html.contains(">Location<"), "heading missing in: {html}");
    assert!(cell(&html, "City").contains("Berlin"));
    let region = cell(&html, "Region");
    assert!(
        region.contains("Berlin") && region.contains("BE"),
        "{region}"
    );
    let country = cell(&html, "Country");
    assert!(
        country.contains("Germany") && country.contains("DE"),
        "{country}"
    );
    assert!(cell(&html, "Continent").contains("Europe"));
    assert!(cell(&html, "Postal code").contains("10115"));
    assert!(cell(&html, "Timezone").contains("Europe/Berlin"));
}

#[test]
fn c9_c3_coordinates_cell_and_map_link() {
    let with = html_of(vec![
        ("latitude", text("52.52")),
        ("longitude", text("13.4")),
        ("accuracy_km", Value::Int(20)),
    ]);
    assert!(
        cell(&with, "Coordinates").contains("52.52, 13.4 ± 20 km"),
        "in: {with}"
    );
    assert!(with.contains(MAP_HREF), "map href missing in: {with}");
    assert!(
        with.contains("rel=\"noopener noreferrer\""),
        "rel missing in: {with}"
    );
    assert!(
        with.contains(">Open in OpenStreetMap<"),
        "link text missing in: {with}"
    );

    let without = html_of(vec![
        ("latitude", text("52.52")),
        ("longitude", text("13.4")),
    ]);
    let coords = cell(&without, "Coordinates");
    assert!(coords.contains("52.52, 13.4"), "{coords}");
    assert!(!coords.contains('±'), "{coords}");
}

#[test]
fn c10_no_coordinates_no_map_link() {
    let html = html_of(vec![("city", text("Berlin"))]);
    assert!(!html.contains("openstreetmap"), "link present in: {html}");
    assert!(
        !html.contains(&label("Coordinates")),
        "label present in: {html}"
    );
    assert!(
        html.contains(&label("City")),
        "city label missing in: {html}"
    );
}

#[test]
fn c11_registered_country_only_when_it_differs() {
    let same = html_of(vec![
        ("country", text("Germany")),
        ("registered_country", text("Germany")),
    ]);
    assert!(!same.contains(&label("Registered country")), "in: {same}");

    let differs = html_of(vec![
        ("country", text("Germany")),
        ("registered_country", text("United States")),
        ("registered_country_iso", text("US")),
    ]);
    assert!(
        cell(&differs, "Registered country").contains("United States"),
        "in: {differs}"
    );
}
