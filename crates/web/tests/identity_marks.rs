//! Contract test for the small identity marks (V2 spec v2-design-system, requirement 8, C2):
//! `static/favicon.svg` and `static/badge-mark.svg` are the marker crossing the axis knocked out
//! of a solid tile (tile radius at most 2 px) by one even-odd path. Geometry only; colours are
//! covered by identity_colours.rs.
use std::collections::HashMap;

/// (file, tile size T, icon size S, inset O): the canvas `throughIcon(S)` placed at O.
const ROWS: [(&str, f64, f64, f64); 2] = [
    ("favicon.svg", 32.0, 20.0, 6.0),
    ("badge-mark.svg", 16.0, 11.0, 2.5),
];

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

type Pt = (f64, f64);

struct Path {
    subpaths: Vec<Vec<Pt>>,
    max_arc_radius: f64,
}

fn numbers(s: &str) -> Vec<f64> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, out: &mut Vec<f64>| {
        if !cur.is_empty() {
            out.push(cur.parse().unwrap_or_else(|_| panic!("bad number {cur:?}")));
            cur.clear();
        }
    };
    for c in s.chars() {
        if c == '-' {
            flush(&mut cur, &mut out);
            cur.push(c);
        } else if c.is_ascii_digit() || c == '.' {
            cur.push(c);
        } else {
            flush(&mut cur, &mut out);
        }
    }
    flush(&mut cur, &mut out);
    out
}

/// Absolute M, L, H, V, A, Z only. An arc is approximated by its chord.
fn parse_path(d: &str, file: &str) -> Path {
    let mut chunks: Vec<(char, String)> = Vec::new();
    for c in d.chars() {
        if c.is_ascii_alphabetic() {
            assert!(
                c.is_ascii_uppercase() && "MLHVAZ".contains(c),
                "{file}: path command `{c}` unsupported; the contract allows absolute M L H V A Z only"
            );
            chunks.push((c, String::new()));
        } else {
            chunks
                .last_mut()
                .unwrap_or_else(|| panic!("{file}: path data starts with a number"))
                .1
                .push(c);
        }
    }
    let mut subpaths: Vec<Vec<Pt>> = Vec::new();
    let mut max_arc_radius = 0.0_f64;
    let mut cur: Pt = (0.0, 0.0);
    let mut start: Pt = (0.0, 0.0);
    for (cmd, args) in chunks {
        let n = numbers(&args);
        let (arity, stride) = match cmd {
            'M' | 'L' => (2, 2),
            'H' | 'V' => (1, 1),
            'A' => (7, 7),
            _ => (0, 1),
        };
        assert!(
            n.len().is_multiple_of(stride) && (cmd == 'Z' || !n.is_empty()),
            "{file}: wrong argument count for `{cmd}`"
        );
        if cmd == 'Z' {
            cur = start;
            continue;
        }
        for (i, a) in n.chunks(arity).enumerate() {
            cur = match cmd {
                'M' | 'L' => (a[0], a[1]),
                'H' => (a[0], cur.1),
                'V' => (cur.0, a[0]),
                _ => {
                    max_arc_radius = max_arc_radius.max(a[0]).max(a[1]);
                    (a[5], a[6])
                }
            };
            if cmd == 'M' && i == 0 {
                start = cur;
                subpaths.push(vec![cur]);
            } else {
                subpaths
                    .last_mut()
                    .unwrap_or_else(|| panic!("{file}: path does not start with M"))
                    .push(cur);
            }
        }
    }
    Path {
        subpaths,
        max_arc_radius,
    }
}

fn inside(poly: &[Pt], (px, py): Pt) -> bool {
    let mut in_ = false;
    for i in 0..poly.len() {
        let (x1, y1) = poly[i];
        let (x2, y2) = poly[(i + 1) % poly.len()];
        if (y1 > py) != (y2 > py) && px < x1 + (py - y1) / (y2 - y1) * (x2 - x1) {
            in_ = !in_;
        }
    }
    in_
}

fn filled(path: &Path, p: Pt) -> bool {
    path.subpaths.iter().filter(|s| inside(s, p)).count() % 2 == 1
}

fn only(src: &str, name: &str, id: &str, file: &str) -> Option<Attrs> {
    let found: Vec<Attrs> = tags(src, name)
        .into_iter()
        .filter(|a| a.get("id").map(String::as_str) == Some(id))
        .collect();
    assert!(
        found.len() <= 1,
        "{file}: expected at most one <{name} id=\"{id}\">"
    );
    found.into_iter().next()
}

#[test]
fn favicon_and_badge_mark_knock_the_marker_crossing_the_axis_out_of_the_tile() {
    for (file, t, s, o) in ROWS {
        let path = format!("{}/static/{file}", env!("CARGO_MANIFEST_DIR"));
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{file}: cannot read {path}: {e}"));

        // Root: viewBox "0 0 T T", width/height T.
        let svg = tags(&src, "svg");
        assert_eq!(svg.len(), 1, "{file}: expected one <svg> root");
        assert_eq!(
            svg[0].get("viewBox").map(String::as_str),
            Some(format!("0 0 {t} {t}").as_str()),
            "{file}: viewBox"
        );
        assert_eq!(num(&svg[0], "width", file), t, "{file}: width");
        assert_eq!(num(&svg[0], "height", file), t, "{file}: height");

        assert!(
            tags(&src, "rect").is_empty(),
            "{file}: no <rect> may remain; the marks are the tile path's cut-out"
        );

        // Tile: one even-odd path, at least two subpaths, first covers the viewBox.
        let tile_attrs = only(&src, "path", "tile", file)
            .unwrap_or_else(|| panic!("{file}: expected exactly one <path id=\"tile\">"));
        assert_eq!(
            tile_attrs.get("fill-rule").map(String::as_str),
            Some("evenodd"),
            "{file}: tile fill-rule"
        );
        let d = tile_attrs
            .get("d")
            .unwrap_or_else(|| panic!("{file}: tile has no d"));
        let tile = parse_path(d, file);
        assert!(
            tile.subpaths.len() >= 2,
            "{file}: tile path needs the tile plus cut-out subpaths, found {}",
            tile.subpaths.len()
        );
        assert!(
            tile.max_arc_radius <= 2.0,
            "{file}: tile arc radius {} exceeds 2 px",
            tile.max_arc_radius
        );
        let first = &tile.subpaths[0];
        let (minx, maxx) = first.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
            (lo.min(p.0), hi.max(p.0))
        });
        let (miny, maxy) = first.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
            (lo.min(p.1), hi.max(p.1))
        });
        assert_eq!(
            (minx, miny, maxx, maxy),
            (0.0, 0.0, t, t),
            "{file}: first subpath must cover the viewBox"
        );

        // Canvas throughIcon(S) at inset O.
        let thick = f64::max(1.0, 0.07 * s);
        let w = f64::max(1.0, 0.06 * s);
        let m = f64::max(2.0, 0.14 * s);
        let ay = o + 0.66 * s; // axis top
        let a_mid = ay + thick / 2.0;
        let a_bot = ay + thick;
        let tick_bot = ay + 0.16 * s;
        let low_mid = (a_bot + tick_bot) / 2.0; // lower part of a tick
        let ticks: Vec<f64> = [0.03, 0.22, 0.8].iter().map(|k| o + k * s).collect();
        let mx = o + 0.5 * s; // marker x-centre
        let m_left = mx - m / 2.0;
        let m_top = o + 0.06 * s;
        let m_bot = m_top + 0.88 * s;
        let above = (m_top + ay) / 2.0;
        let below = (a_bot + m_bot) / 2.0;
        let gap_x = (ticks[0] + w + ticks[1]) / 2.0;

        let mut holes: Vec<(&str, Pt)> = vec![
            ("marker above axis", (mx, above)),
            ("marker below axis", (mx, below)),
            ("marker/axis crossing", (mx, a_mid)),
            ("axis between tick 1 and 2", (gap_x, a_mid)),
        ];
        for (i, x) in ticks.iter().enumerate() {
            let name = [
                "tick 1 lower part",
                "tick 2 lower part",
                "tick 3 lower part",
            ][i];
            holes.push((name, (x + w / 2.0, low_mid)));
        }
        let solids: Vec<(&str, Pt)> = vec![
            ("above the icon", (t / 2.0, o / 2.0)),
            ("left of the icon", (o / 2.0, t / 2.0)),
            ("right of the icon", (t - o / 2.0, t / 2.0)),
            ("between tick 1 and 2 below axis", (gap_x, low_mid)),
            (
                "left of marker above axis",
                (m_left - (m_left - o) / 2.0, above),
            ),
        ];
        for (name, p) in &holes {
            assert!(
                !filled(&tile, *p),
                "{file}: {name} at {p:?} must be cut out (not filled)"
            );
        }
        for (name, p) in &solids {
            assert!(
                filled(&tile, *p),
                "{file}: {name} at {p:?} must be solid (filled)"
            );
        }

        // Badge mark: a second path fills the cut-out; the favicon has none.
        let mark = only(&src, "path", "mark", file);
        if file == "badge-mark.svg" {
            let mark = mark.unwrap_or_else(|| panic!("{file}: expected one <path id=\"mark\">"));
            let mark = parse_path(mark.get("d").map_or("", String::as_str), file);
            for (name, p) in &holes {
                assert!(
                    filled(&mark, *p),
                    "{file}: mark path must fill {name} at {p:?}"
                );
            }
        } else {
            assert!(mark.is_none(), "{file}: must not have a <path id=\"mark\">");
        }
    }
}
