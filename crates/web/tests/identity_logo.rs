//! Contract test for the W5b wordmark (V2 spec v2-design-system, requirement 8, criterion C1).
//! Structure only: texts and weights, the two scales, the marker crossing the ray axis.
//! Colours and the font stack are covered by identity_colours.rs.

/// One element: attributes by name, plus the raw text up to the closing tag (empty if self-closing).
struct Elem {
    attrs: Vec<(String, String)>,
    inner: String,
}

impl Elem {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    fn num(&self, name: &str) -> f64 {
        let raw = self
            .attr(name)
            .unwrap_or_else(|| panic!("missing attribute {name} on {:?}", self.attrs));
        raw.parse()
            .unwrap_or_else(|_| panic!("attribute {name}={raw:?} is not numeric"))
    }
}

fn parse_attrs(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = tag;
    while let Some(eq) = rest.find("=\"") {
        let name = rest[..eq]
            .rsplit(|c: char| c.is_whitespace() || c == '<')
            .next()
            .unwrap_or("")
            .to_string();
        let after = &rest[eq + 2..];
        let Some(end) = after.find('"') else { break };
        out.push((name, after[..end].to_string()));
        rest = &after[end + 1..];
    }
    out
}

/// All `<name ...>` elements in document order (no nesting of the same name assumed).
fn elements(src: &str, name: &str) -> Vec<Elem> {
    let open = format!("<{name}");
    let close = format!("</{name}>");
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(i) = src[pos..].find(&open) {
        let start = pos + i;
        let after = start + open.len();
        let next = src[after..].chars().next().unwrap_or(' ');
        if !(next.is_whitespace() || next == '>' || next == '/') {
            pos = after;
            continue;
        }
        let tag_end = start + src[start..].find('>').expect("unterminated tag");
        let tag = &src[start..tag_end];
        let (inner, resume) = if tag.ends_with('/') {
            (String::new(), tag_end + 1)
        } else {
            let c = src[tag_end..]
                .find(&close)
                .map_or(src.len(), |j| tag_end + j);
            (src[tag_end + 1..c].to_string(), c)
        };
        out.push(Elem {
            attrs: parse_attrs(tag),
            inner,
        });
        pos = resume;
    }
    out
}

fn logo() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/static/logo.svg"))
        .expect("static/logo.svg must exist")
}

fn text_named(src: &str, content: &str) -> Elem {
    let mut found: Vec<Elem> = elements(src, "text")
        .into_iter()
        .filter(|t| t.inner.trim() == content)
        .collect();
    assert_eq!(
        found.len(),
        1,
        "exactly one <text> with content {content:?}"
    );
    found.remove(0)
}

/// The inner markup of `<g id="...">`.
fn group(src: &str, id: &str) -> String {
    let mut found: Vec<Elem> = elements(src, "g")
        .into_iter()
        .filter(|g| g.attr("id") == Some(id))
        .collect();
    assert_eq!(found.len(), 1, "exactly one <g id={id:?}>");
    found.remove(0).inner
}

/// The axis rect and the tick count of a scale group.
fn scale(src: &str, id: &str) -> (Elem, usize) {
    let inner = group(src, id);
    let rects = elements(&inner, "rect");
    let mut axes: Vec<Elem> = Vec::new();
    let mut ticks = 0;
    for r in rects {
        match r.attr("class") {
            Some("axis") => axes.push(r),
            Some("tick") => ticks += 1,
            _ => {}
        }
    }
    assert_eq!(axes.len(), 1, "{id} holds exactly one axis rect");
    (axes.remove(0), ticks)
}

#[test]
fn wordmark_has_net_regular_ray_bold() {
    let src = logo();
    let net = text_named(&src, "net");
    let ray = text_named(&src, "ray");
    assert_eq!(net.attr("font-weight"), Some("400"), "net weight");
    assert_eq!(ray.attr("font-weight"), Some("700"), "ray weight");
    assert!(net.num("x") < ray.num("x"), "net sits left of ray");
    // y must be numeric for both
    net.num("y");
    ray.num("y");
}

#[test]
fn each_word_has_a_scale_with_its_tick_count() {
    let src = logo();
    for (id, ticks) in [("scale-net", 6), ("scale-ray", 5)] {
        let (_, got) = scale(&src, id);
        assert_eq!(got, ticks, "tick rects in {id}");
    }
}

#[test]
fn axes_lie_below_the_text_baselines() {
    let src = logo();
    for (id, word) in [("scale-net", "net"), ("scale-ray", "ray")] {
        let (axis, _) = scale(&src, id);
        let text = text_named(&src, word);
        assert!(
            axis.num("y") > text.num("y"),
            "{id} axis must lie below the {word} baseline"
        );
    }
}

#[test]
fn marker_is_a_vertical_bar_crossing_the_ray_axis_between_the_texts() {
    let src = logo();
    let markers: Vec<Elem> = elements(&src, "rect")
        .into_iter()
        .filter(|r| r.attr("id") == Some("marker"))
        .collect();
    assert_eq!(markers.len(), 1, "exactly one rect#marker");
    let m = &markers[0];
    assert!(m.num("height") > m.num("width"), "marker is vertical");

    let (axis, _) = scale(&src, "scale-ray");
    assert!(
        m.num("y") < axis.num("y"),
        "marker starts above the ray axis"
    );
    assert!(
        m.num("y") + m.num("height") > axis.num("y") + axis.num("height"),
        "marker ends below the ray axis"
    );

    let net = text_named(&src, "net");
    let ray = text_named(&src, "ray");
    assert!(
        net.num("x") < m.num("x") && m.num("x") < ray.num("x"),
        "marker sits between the net and ray texts"
    );
}
