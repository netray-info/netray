//! Contract test for the small identity marks (V2 spec v2-design-system, requirement 8, C2):
//! `static/favicon.svg` and `static/badge-mark.svg` are the marker crossing the axis on a tile
//! with at most 2 px radius. Structure only; colours are covered by identity_colours.rs.
use std::collections::HashMap;

const FILES: [&str; 2] = ["favicon.svg", "badge-mark.svg"];

type Attrs = HashMap<String, String>;

/// Attributes of every `<name ...>` tag in `src`, parsed by name (`name="value"` pairs).
fn tags(src: &str, name: &str) -> Vec<Attrs> {
    let open = format!("<{name}");
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(i) = rest.find(&open) {
        let after = &rest[i + open.len()..];
        // `<rect` must not match `<rectangle`: the next char has to end the tag name.
        let boundary = after
            .chars()
            .next()
            .is_some_and(|c| c.is_whitespace() || c == '/' || c == '>');
        let end = after.find('>').unwrap_or(after.len());
        if boundary {
            out.push(attrs(&after[..end]));
        }
        rest = &after[end..];
    }
    out
}

fn attrs(tag: &str) -> Attrs {
    let mut out = Attrs::new();
    let mut rest = tag;
    while let Some(eq) = rest.find("=\"") {
        let key = rest[..eq]
            .rsplit(|c: char| c.is_whitespace())
            .next()
            .unwrap_or("")
            .to_string();
        let val_start = eq + 2;
        let Some(len) = rest[val_start..].find('"') else {
            break;
        };
        out.insert(key, rest[val_start..val_start + len].to_string());
        rest = &rest[val_start + len + 1..];
    }
    out
}

fn num(a: &Attrs, key: &str, what: &str) -> f64 {
    a.get(key)
        .unwrap_or_else(|| panic!("{what}: missing attribute `{key}`"))
        .parse()
        .unwrap_or_else(|_| panic!("{what}: `{key}` is not a plain number: {:?}", a[key]))
}

struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

fn rect(a: &Attrs, what: &str) -> Rect {
    Rect {
        x: num(a, "x", what),
        y: num(a, "y", what),
        w: num(a, "width", what),
        h: num(a, "height", what),
    }
}

fn by_id<'a>(rects: &'a [Attrs], id: &str, file: &str) -> &'a Attrs {
    let found: Vec<_> = rects
        .iter()
        .filter(|a| a.get("id").map(String::as_str) == Some(id))
        .collect();
    assert_eq!(
        found.len(),
        1,
        "{file}: expected exactly one <rect id=\"{id}\">"
    );
    found[0]
}

#[test]
fn favicon_and_badge_mark_are_the_marker_crossing_the_axis_on_a_tile() {
    for file in FILES {
        let path = format!("{}/static/{file}", env!("CARGO_MANIFEST_DIR"));
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{file}: cannot read {path}: {e}"));

        // Root: viewBox "0 0 W H", width/height equal to it, so a user unit is a px.
        let svg = tags(&src, "svg");
        assert_eq!(svg.len(), 1, "{file}: expected one <svg> root");
        let vb: Vec<f64> = svg[0]
            .get("viewBox")
            .unwrap_or_else(|| panic!("{file}: root has no viewBox"))
            .split_whitespace()
            .map(|n| {
                n.parse()
                    .unwrap_or_else(|_| panic!("{file}: bad viewBox number {n:?}"))
            })
            .collect();
        assert_eq!(vb.len(), 4, "{file}: viewBox needs four numbers");
        assert_eq!((vb[0], vb[1]), (0.0, 0.0), "{file}: viewBox starts at 0 0");
        assert_eq!(
            num(&svg[0], "width", file),
            vb[2],
            "{file}: width equals viewBox width"
        );
        assert_eq!(
            num(&svg[0], "height", file),
            vb[3],
            "{file}: height equals viewBox height"
        );

        let rects = tags(&src, "rect");

        // Tile: covers the viewBox, radius at most 2 px (absent counts as 0).
        let tile_attrs = by_id(&rects, "tile", file);
        let tile = rect(tile_attrs, &format!("{file} tile"));
        assert_eq!(
            (tile.x, tile.y, tile.w, tile.h),
            (0.0, 0.0, vb[2], vb[3]),
            "{file}: tile covers the viewBox"
        );
        for r in ["rx", "ry"] {
            if tile_attrs.contains_key(r) {
                let v = num(tile_attrs, r, &format!("{file} tile"));
                assert!(
                    (0.0..=2.0).contains(&v),
                    "{file}: tile {r}={v} exceeds 2 px"
                );
            }
        }

        // Axis: horizontal.
        let axis = rect(by_id(&rects, "axis", file), &format!("{file} axis"));
        assert!(
            axis.w > axis.h,
            "{file}: axis is horizontal (width > height)"
        );

        // Exactly three ticks.
        let ticks: Vec<_> = rects
            .iter()
            .filter(|a| {
                a.get("class")
                    .is_some_and(|c| c.split_whitespace().any(|t| t == "tick"))
            })
            .collect();
        assert_eq!(ticks.len(), 3, "{file}: expected exactly 3 tick rects");

        // Marker: vertical and crossing the axis, within the axis extent.
        let marker = rect(by_id(&rects, "marker", file), &format!("{file} marker"));
        assert!(
            marker.h > marker.w,
            "{file}: marker is vertical (height > width)"
        );
        assert!(
            marker.y < axis.y && marker.y + marker.h > axis.y + axis.h,
            "{file}: marker must cross the axis vertically"
        );
        assert!(
            axis.x <= marker.x && marker.x + marker.w <= axis.x + axis.w,
            "{file}: marker must lie within the axis extent"
        );

        // Every rect inside the tile.
        for (i, a) in rects.iter().enumerate() {
            let r = rect(a, &format!("{file} rect #{i}"));
            assert!(
                r.x >= tile.x
                    && r.y >= tile.y
                    && r.x + r.w <= tile.x + tile.w
                    && r.y + r.h <= tile.y + tile.h,
                "{file}: rect #{i} ({:?}) lies outside the tile",
                a.get("id").or(a.get("class"))
            );
        }
    }
}
