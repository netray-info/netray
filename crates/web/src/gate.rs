//! The design-system gate (SDD §8.5): scans the view layer's templates, stylesheets and islands
//! for colour, radius, font and line-width literals outside the tokens, for a second
//! implementation of an R44 component, and for inline scripts.
//!
//! Every file is reduced to the text a rule may look at before it is scanned: comments are
//! blanked (same length, so offsets and line numbers stay true), and a template is cut into its
//! tags, `style` attributes and `<style>` bodies.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::tokens;

/// Rule 1: a literal where a token belongs.
pub const TOKEN_RULE: u8 = 1;
/// Rule 2: an R44 component's class outside its own macro file or stylesheet.
pub const OWNERSHIP_RULE: u8 = 2;
/// Rule 3: an inline script other than the speculation rules.
pub const SCRIPT_RULE: u8 = 3;

/// The R44 components; a component's CSS class is its name with `_` turned into `-`.
const COMPONENTS: [&str; 10] = [
    "grade",
    "run_line",
    "progress_ray",
    "spectrum",
    "trace",
    "check_row",
    "evidence_block",
    "diff_block",
    "grouping_switch",
    "query_input",
];

const COLOUR_FUNCTIONS: [&str; 11] = [
    "rgb",
    "rgba",
    "hsl",
    "hsla",
    "hwb",
    "lab",
    "lch",
    "oklab",
    "oklch",
    "color",
    "color-mix",
];

const BORDER_STYLES: [&str; 10] = [
    "none", "hidden", "solid", "dashed", "dotted", "double", "groove", "ridge", "inset", "outset",
];

const CSS_WIDE: [&str; 5] = ["inherit", "initial", "unset", "revert", "revert-layer"];

/// SVG presentation attributes that carry a colour.
const SVG_COLOUR_ATTRS: [&str; 6] = [
    "fill",
    "stroke",
    "stop-color",
    "flood-color",
    "lighting-color",
    "color",
];

/// SVG presentation attributes whose value must be made of tokens.
const SVG_TOKEN_ATTRS: [&str; 5] = ["stroke-width", "font-size", "font-family", "rx", "ry"];

/// Properties whose value is made of identifiers or strings, where a word that is also a CSS
/// colour name (`mark`, `highlight`, `red`) is no colour. Every other property, and every custom
/// property, may hold a colour.
const IDENTIFIER_PROPERTIES: [&str; 29] = [
    "grid-area",
    "grid-row",
    "grid-row-start",
    "grid-row-end",
    "grid-column",
    "grid-column-start",
    "grid-column-end",
    "grid-template-areas",
    "animation-name",
    "view-transition-name",
    "transition-property",
    "will-change",
    "counter-reset",
    "counter-increment",
    "counter-set",
    "content",
    "quotes",
    "font-family",
    "font-feature-settings",
    "list-style-type",
    "container-name",
    "anchor-name",
    "position-anchor",
    "scroll-timeline-name",
    "view-timeline-name",
    "timeline-scope",
    "page",
    "text-transform",
    "container",
];

/// The CSS named colours and system colours, lower case (`transparent` and `currentcolor` are no
/// palette entry and stay allowed).
const NAMED_COLOURS: &str = "aliceblue antiquewhite aqua aquamarine azure beige bisque black \
    blanchedalmond blue blueviolet brown burlywood cadetblue chartreuse chocolate coral \
    cornflowerblue cornsilk crimson cyan darkblue darkcyan darkgoldenrod darkgray darkgreen \
    darkgrey darkkhaki darkmagenta darkolivegreen darkorange darkorchid darkred darksalmon \
    darkseagreen darkslateblue darkslategray darkslategrey darkturquoise darkviolet deeppink \
    deepskyblue dimgray dimgrey dodgerblue firebrick floralwhite forestgreen fuchsia gainsboro \
    ghostwhite gold goldenrod gray green greenyellow grey honeydew hotpink indianred indigo ivory \
    khaki lavender lavenderblush lawngreen lemonchiffon lightblue lightcoral lightcyan \
    lightgoldenrodyellow lightgray lightgreen lightgrey lightpink lightsalmon lightseagreen \
    lightskyblue lightslategray lightslategrey lightsteelblue lightyellow lime limegreen linen \
    magenta maroon mediumaquamarine mediumblue mediumorchid mediumpurple mediumseagreen \
    mediumslateblue mediumspringgreen mediumturquoise mediumvioletred midnightblue mintcream \
    mistyrose moccasin navajowhite navy oldlace olive olivedrab orange orangered orchid \
    palegoldenrod palegreen paleturquoise palevioletred papayawhip peachpuff peru pink plum \
    powderblue purple rebeccapurple red rosybrown royalblue saddlebrown salmon sandybrown \
    seagreen seashell sienna silver skyblue slateblue slategray slategrey snow springgreen \
    steelblue tan teal thistle tomato turquoise violet wheat white whitesmoke yellow yellowgreen \
    canvas canvastext linktext visitedtext activetext buttonface buttontext buttonborder \
    field fieldtext highlight highlighttext selecteditem selecteditemtext mark marktext graytext \
    accentcolor accentcolortext activeborder activecaption appworkspace background \
    buttonhighlight buttonshadow captiontext inactiveborder inactivecaption inactivecaptiontext \
    infobackground infotext menu menutext scrollbar threeddarkshadow threedface threedhighlight \
    threedlightshadow threedshadow window windowframe windowtext";

/// One breach of a gate rule.
#[derive(Debug)]
pub struct Violation {
    /// `TOKEN_RULE`, `OWNERSHIP_RULE` or `SCRIPT_RULE`.
    pub rule: u8,
    /// The file, relative to the scanned root.
    pub path: PathBuf,
    /// 1-based line of the breach.
    pub line: usize,
    pub message: String,
}

/// The outcome of a scan: how many files were looked at, and what they broke.
#[derive(Debug)]
pub struct Report {
    pub scanned: usize,
    pub violations: Vec<Violation>,
}

/// A breach before it has a file and a line: rule, byte offset into the file, message.
type Finding = (u8, usize, String);

/// The tokens `tokens::CSS` defines.
struct Tokens {
    /// Every custom property it declares.
    all: HashSet<String>,
    /// The subset whose value holds a colour.
    colours: HashSet<String>,
}

impl Tokens {
    fn load(css: &str) -> Self {
        let (mut all, mut colours) = (HashSet::new(), HashSet::new());
        for (_, prop, value) in declarations(css, 0) {
            if prop.starts_with("--") {
                if !colour_hits(value, true).is_empty() {
                    colours.insert(prop.clone());
                }
                all.insert(prop);
            }
        }
        Tokens { all, colours }
    }
}

/// Scans `templates/**/*.html`, `static/**/*.css` and `islands/**/*.ts` under `root`; a missing
/// directory is empty and SVGs are never scanned.
pub fn check(root: &Path) -> Report {
    let tokens = Tokens::load(tokens::CSS);
    let mut report = Report {
        scanned: 0,
        violations: Vec::new(),
    };
    for (dir, ext) in [("templates", "html"), ("static", "css"), ("islands", "ts")] {
        let mut files = Vec::new();
        walk(&root.join(dir), ext, &mut files);
        files.sort();
        for path in files {
            let text = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            let rel = path.strip_prefix(root).unwrap_or(&path);
            report.scanned += 1;
            report.violations.extend(scan(rel, &text, ext, &tokens));
        }
    }
    report
}

fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|e| e.path()) {
        if path.is_dir() {
            walk(&path, ext, out);
        } else if path.extension().is_some_and(|e| e == ext) {
            out.push(path);
        }
    }
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// All violations of one file; `rel` is its path under the scanned root, `ext` its kind.
fn scan(rel: &Path, text: &str, ext: &str, tokens: &Tokens) -> Vec<Violation> {
    let mut found = Vec::new();
    match ext {
        "css" => {
            let (css, _) = lex(text, false);
            scan_css(&css, 0, tokens, &mut found);
            let own = |name: &str| rel == Path::new(&format!("static/{name}.css"));
            owned_selectors(&css, 0, own, &mut found);
        }
        "html" => scan_template(rel, text, tokens, &mut found),
        _ => scan_island(text, &mut found),
    }
    found
        .into_iter()
        .map(|(rule, offset, message)| Violation {
            rule,
            path: rel.to_path_buf(),
            line: 1 + text[..offset].matches('\n').count(),
            message,
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Reducing a file to what a rule may look at
// ---------------------------------------------------------------------------------------------

/// `s` with every character but newlines replaced by `fill` (ASCII), byte for byte.
fn blanked(s: &str, fill: char) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '\n' {
            out.push('\n');
        } else {
            out.extend(std::iter::repeat_n(fill, c.len_utf8()));
        }
    }
    out
}

/// `text` with each `open … close` span blanked; an unclosed span runs to the end.
fn blank(text: &str, open: &str, close: &str, fill: char) -> String {
    let (mut out, mut rest) = (String::with_capacity(text.len()), text);
    while let Some(i) = rest.find(open) {
        let after = i + open.len();
        let end = rest[after..]
            .find(close)
            .map_or(rest.len(), |n| after + n + close.len());
        out.push_str(&rest[..i]);
        out.push_str(&blanked(&rest[i..end], fill));
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// The `(start, end)` offsets of each `open … close` span of `text`; an unclosed span runs to the
/// end.
fn spans(text: &str, open: &str, close: &str) -> Vec<(usize, usize)> {
    let (mut out, mut at) = (Vec::new(), 0);
    while let Some(i) = text[at..].find(open).map(|n| at + n) {
        let after = i + open.len();
        let end = text[after..]
            .find(close)
            .map_or(text.len(), |n| after + n + close.len());
        out.push((i, end));
        at = end;
    }
    out
}

/// Lexes CSS or island source: the text with its comments blanked (`/* */`, and `//` when
/// `line_comments`), and the string literals (`'…'`, `"…"`, and `` `…` `` across lines) as
/// `(offset of the content, content)`.
fn lex(text: &str, line_comments: bool) -> (String, Vec<(usize, &str)>) {
    let bytes = text.as_bytes();
    let (mut code, mut strings) = (String::with_capacity(text.len()), Vec::new());
    let (mut i, mut kept) = (0, 0);
    while i < bytes.len() {
        let comment_end = match (bytes[i], bytes.get(i + 1)) {
            (b'/', Some(b'*')) => Some(text[i + 2..].find("*/").map_or(bytes.len(), |n| i + 4 + n)),
            (b'/', Some(b'/')) if line_comments => {
                Some(text[i..].find('\n').map_or(bytes.len(), |n| i + n))
            }
            _ => None,
        };
        if let Some(end) = comment_end {
            code.push_str(&text[kept..i]);
            code.push_str(&blanked(&text[i..end], ' '));
            (i, kept) = (end, end);
        } else if matches!(bytes[i], b'\'' | b'"' | b'`') {
            let quote = bytes[i];
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != quote && (quote == b'`' || bytes[j] != b'\n') {
                j += if bytes[j] == b'\\' { 2 } else { 1 };
            }
            let j = j.min(bytes.len());
            strings.push((i + 1, &text[i + 1..j]));
            i = j + 1;
        } else {
            i += 1;
        }
    }
    code.push_str(&text[kept.min(text.len())..]);
    (code, strings)
}

// ---------------------------------------------------------------------------------------------
// Templates: tags, attributes, bodies
// ---------------------------------------------------------------------------------------------

struct Attr<'a> {
    /// Lower case.
    name: String,
    /// Offset and content of the value, if there is one.
    value: Option<(usize, &'a str)>,
}

struct Tag<'a> {
    /// Lower case.
    name: String,
    /// Offset of the `<`.
    start: usize,
    attrs: Vec<Attr<'a>>,
    /// Offset and content of a `<style>` or `<script>` element's body.
    body: Option<(usize, &'a str)>,
}

impl<'a> Tag<'a> {
    /// The values of the attributes named exactly `name` (`data-src` is no `src`).
    fn values(&self, name: &str) -> impl Iterator<Item = (usize, &'a str)> {
        let name = name.to_string();
        self.attrs
            .iter()
            .filter(move |a| a.name == name)
            .filter_map(|a| a.value)
    }
}

/// Every start tag in `text`; the body of a `<style>` or `<script>` is not scanned for tags.
fn tags(text: &str) -> Vec<Tag<'_>> {
    let (bytes, mut out, mut i) = (text.as_bytes(), Vec::new(), 0);
    while let Some(start) = text[i..].find('<').map(|n| i + n) {
        let name_at = start + 1;
        let name_len = text[name_at..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .unwrap_or(text.len() - name_at);
        if name_len == 0 || !bytes[name_at].is_ascii_alphabetic() {
            i = name_at;
            continue;
        }
        let name = text[name_at..name_at + name_len].to_ascii_lowercase();
        let attrs_at = name_at + name_len;
        let end = tag_end(text, attrs_at);
        let mut tag = Tag {
            attrs: parse_attrs(text, attrs_at, end),
            body: None,
            name,
            start,
        };
        i = (end + 1).min(text.len());
        if tag.name == "style" || tag.name == "script" {
            let close = format!("</{}", tag.name);
            let body_end = text[i..]
                .to_ascii_lowercase()
                .find(&close)
                .map_or(text.len(), |n| i + n);
            tag.body = Some((i, &text[i..body_end]));
            i = body_end;
        }
        out.push(tag);
    }
    out
}

/// The offset of the `>` that ends the tag whose attributes start at `from`, outside quotes.
fn tag_end(text: &str, from: usize) -> usize {
    let mut quote = None;
    for (j, c) in text[from..].char_indices() {
        match (quote, c) {
            (None, '>') => return from + j,
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), _) if c == q => quote = None,
            _ => {}
        }
    }
    text.len()
}

fn parse_attrs(text: &str, from: usize, to: usize) -> Vec<Attr<'_>> {
    let (bytes, mut out, mut i) = (text.as_bytes(), Vec::new(), from);
    let skip_space = |mut j: usize| {
        while j < to && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        j
    };
    while i < to {
        if bytes[i].is_ascii_whitespace() || matches!(bytes[i], b'/' | b'=') {
            i += 1;
            continue;
        }
        let name_at = i;
        while i < to && !bytes[i].is_ascii_whitespace() && !matches!(bytes[i], b'=' | b'/') {
            i += 1;
        }
        let name = text[name_at..i].to_ascii_lowercase();
        let eq = skip_space(i);
        if eq >= to || bytes[eq] != b'=' {
            out.push(Attr { name, value: None });
            continue;
        }
        let at = skip_space(eq + 1);
        let (value_at, value_end) = if at < to && matches!(bytes[at], b'"' | b'\'') {
            let end = text[at + 1..to]
                .find(char::from(bytes[at]))
                .map_or(to, |n| at + 1 + n);
            i = (end + 1).min(to);
            (at + 1, end)
        } else {
            i = at;
            while i < to && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            (at, i)
        };
        out.push(Attr {
            name,
            value: Some((value_at, &text[value_at..value_end])),
        });
    }
    out
}

fn scan_template(rel: &Path, raw: &str, tokens: &Tokens, out: &mut Vec<Finding>) {
    let mut text = blank(raw, "<!--", "-->", ' ');
    let mut askama = Vec::new();
    for (open, close) in [("{#", "#}"), ("{{", "}}"), ("{%", "%}")] {
        askama.extend(spans(&text, open, close));
        text = blank(&text, open, close, ' ');
    }
    for tag in tags(&text) {
        for (at, value) in tag.values("style") {
            scan_css_region(value, at, tokens, out);
        }
        if tag.name == "style"
            && let Some((at, body)) = tag.body
        {
            scan_css_region(body, at, tokens, out);
            let (css, _) = lex(body, false);
            owned_selectors(&css, at, |_| false, out);
        }
        for attr in SVG_COLOUR_ATTRS {
            for (at, value) in tag.values(attr) {
                for (_, lit) in colour_hits(value, true) {
                    out.push((
                        TOKEN_RULE,
                        at,
                        format!("colour literal `{lit}` in `{attr}`"),
                    ));
                }
            }
        }
        for attr in SVG_TOKEN_ATTRS {
            for (at, value) in tag.values(attr) {
                token_findings(attr, value, at, tokens, out);
            }
        }
        for (at, classes) in tag.values("class") {
            owned_classes(rel, classes, at, out);
        }
        for attr in &tag.attrs {
            if is_event_handler(&attr.name) {
                out.push((
                    SCRIPT_RULE,
                    tag.start,
                    format!("inline event handler `{}`", attr.name),
                ));
            }
        }
        for name in ["href", "src", "action", "formaction", "xlink:href"] {
            for (_, value) in tag.values(name) {
                if value
                    .trim_start()
                    .to_ascii_lowercase()
                    .starts_with("javascript:")
                {
                    out.push((
                        SCRIPT_RULE,
                        tag.start,
                        format!("`javascript:` URL in `{name}`"),
                    ));
                }
            }
        }
        if tag.name == "script" {
            let has_src = tag.values("src").any(|(at, v)| {
                !v.trim().is_empty() || askama.iter().any(|&(s, e)| s < at + v.len() && at < e)
            });
            let speculation = tag
                .values("type")
                .any(|(_, v)| v.trim().eq_ignore_ascii_case("speculationrules"));
            if !has_src && !speculation {
                out.push((SCRIPT_RULE, tag.start, "inline `<script>`".to_string()));
            }
        }
    }
}

/// Whether an attribute name has the shape of an event handler: `on` and three or more lower
/// case letters.
fn is_event_handler(name: &str) -> bool {
    name.strip_prefix("on")
        .is_some_and(|rest| rest.len() >= 3 && rest.bytes().all(|b| b.is_ascii_lowercase()))
}

/// Scans a CSS region of a template; its askama delimiters are blanked already.
fn scan_css_region(region: &str, base: usize, tokens: &Tokens, out: &mut Vec<Finding>) {
    let (css, _) = lex(region, false);
    scan_css(&css, base, tokens, out);
}

// ---------------------------------------------------------------------------------------------
// Rule 1: tokens
// ---------------------------------------------------------------------------------------------

/// The `prop: value` declarations in a CSS region as `(value offset, property, value)`.
/// Selectors and at-rule preludes end in `{` and are dropped; the last declaration of a
/// `style=""` attribute needs no terminator. A `;` inside a string or parentheses does not end a
/// declaration. Custom properties keep their case.
fn declarations(region: &str, base: usize) -> Vec<(usize, String, &str)> {
    let mut out = Vec::new();
    let (mut start, mut depth, mut quote) = (0, 0_u32, None);
    for (i, c) in region.char_indices().chain([(region.len(), ';')]) {
        let last = i == region.len();
        if let Some(q) = quote
            && !last
        {
            if c == q || c == '\n' {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            '{' if depth == 0 => start = i + 1,
            ';' | '}' if depth == 0 || last => {
                if let Some((prop, value)) = region[start..i].split_once(':') {
                    let prop = prop.trim();
                    if !prop.is_empty() && prop.chars().all(is_word) {
                        let prop = if prop.starts_with("--") {
                            prop.to_string()
                        } else {
                            prop.to_ascii_lowercase()
                        };
                        out.push((base + i - value.len(), prop, value));
                    }
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    out
}

/// Rule 1 over comment-free CSS: colours anywhere, redefined tokens, and token properties.
fn scan_css(css: &str, base: usize, tokens: &Tokens, out: &mut Vec<Finding>) {
    for (at, prop, value) in declarations(css, base) {
        for (_, lit) in colour_hits(value, colour_bearing(&prop)) {
            out.push((
                TOKEN_RULE,
                at,
                format!("colour literal `{lit}` in `{prop}`"),
            ));
        }
        if prop.starts_with("--") {
            if tokens.all.contains(&prop) {
                out.push((TOKEN_RULE, at, format!("`{prop}` redefines a token")));
            }
        } else {
            token_findings(&prop, value, at, tokens, out);
        }
    }
}

fn is_named_colour(word: &str) -> bool {
    NAMED_COLOURS.split_whitespace().any(|n| n == word)
}

/// `prop` without a leading vendor prefix.
fn unprefixed(prop: &str) -> &str {
    ["-webkit-", "-moz-", "-ms-", "-o-"]
        .iter()
        .find_map(|p| prop.strip_prefix(p))
        .unwrap_or(prop)
}

/// Whether a named or system colour can stand as a value of `prop`: in every property but the
/// identifier-valued ones.
fn colour_bearing(prop: &str) -> bool {
    prop.starts_with("--") || !IDENTIFIER_PROPERTIES.contains(&unprefixed(prop))
}

/// The offset of the `)` that closes a parenthesis opened just before `from`, outside quotes.
fn closing_paren(text: &str, from: usize) -> usize {
    let (mut depth, mut quote) = (1, None);
    for (j, c) in text[from..].char_indices() {
        match (quote, c) {
            (Some(q), _) => {
                if c == q {
                    quote = None;
                }
            }
            (None, '"' | '\'') => quote = Some(c),
            (None, '(') => depth += 1,
            (None, ')') => {
                depth -= 1;
                if depth == 0 {
                    return from + j;
                }
            }
            _ => {}
        }
    }
    text.len()
}

/// `text` with the arguments of its `url( … )` calls blanked, and those arguments that are data
/// URIs as `(offset, argument)`.
fn mask_urls(text: &str) -> (String, Vec<(usize, &str)>) {
    let lower = text.to_ascii_lowercase();
    let (mut out, mut data, mut pos) = (String::with_capacity(text.len()), Vec::new(), 0);
    while let Some(i) = lower[pos..].find("url(").map(|n| pos + n) {
        let args = i + 4;
        out.push_str(&text[pos..args]);
        pos = args;
        if i > 0 && is_word(char::from(text.as_bytes()[i - 1])) {
            continue;
        }
        let end = closing_paren(text, args);
        let arg = text[args..end].trim_start().trim_start_matches(['"', '\'']);
        if arg
            .as_bytes()
            .get(..5)
            .is_some_and(|b| b.eq_ignore_ascii_case(b"data:"))
        {
            data.push((args, &text[args..end]));
        }
        out.push_str(&blanked(&text[args..end], ' '));
        pos = end;
    }
    out.push_str(&text[pos..]);
    (out, data)
}

/// The colours a data URI's text holds: SVG colour attributes and `style` attributes.
fn data_uri_colours(uri: &str) -> Vec<String> {
    let mut out = Vec::new();
    for tag in tags(uri) {
        for attr in SVG_COLOUR_ATTRS {
            for (_, value) in tag.values(attr) {
                out.extend(colour_hits(value, true).into_iter().map(|(_, l)| l));
            }
        }
        for (_, style) in tag.values("style") {
            for (_, prop, value) in declarations(style, 0) {
                let hits = colour_hits(value, colour_bearing(&prop));
                out.extend(hits.into_iter().map(|(_, l)| l));
            }
        }
    }
    out
}

/// Colour literals in `text` as `(offset, literal)`: hex and colour functions anywhere outside a
/// `url()`, named colours only when `named` and as a whole token outside quotes and not as a
/// function name. A data URI's text is scanned as markup.
fn colour_hits(text: &str, named: bool) -> Vec<(usize, String)> {
    let (masked, data) = mask_urls(text);
    let (bytes, mut hits, mut i, mut quote) = (masked.as_bytes(), Vec::new(), 0, None);
    while i < bytes.len() {
        let c = char::from(bytes[i]);
        let mut end = i + 1;
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
        } else if matches!(c, '"' | '\'') {
            quote = Some(c);
        }
        if c == '#' {
            end = i
                + 1
                + masked[i + 1..]
                    .find(|c: char| !c.is_ascii_hexdigit())
                    .unwrap_or(masked.len() - i - 1);
            let digits = end - i - 1;
            let word_follows = masked[end..].chars().next().is_some_and(is_word);
            if matches!(digits, 3 | 4 | 6 | 8) && !word_follows {
                hits.push((i, masked[i..end].to_string()));
            }
        } else if is_word(c) {
            end = i + masked[i..]
                .find(|c: char| !is_word(c))
                .unwrap_or(masked.len() - i);
            let word = masked[i..end].to_ascii_lowercase();
            let call = masked[end..].starts_with('(');
            let function = call && COLOUR_FUNCTIONS.contains(&word.as_str());
            let name = named && quote.is_none() && !call && is_named_colour(&word);
            if function || name {
                hits.push((i, masked[i..end].to_string()));
            }
        }
        i = end;
    }
    for (at, uri) in data {
        hits.extend(data_uri_colours(uri).into_iter().map(|lit| (at, lit)));
    }
    hits
}

/// The properties whose value must be made of tokens, by the token family they draw from.
#[derive(Clone, Copy, PartialEq)]
enum Family {
    Radius,
    FontSize,
    /// `font-family`: the typefaces.
    Face,
    Font,
    /// Stroke width and the border widths.
    Width,
    /// The border shorthands: a width, a style and a colour.
    Border,
}

impl Family {
    fn of(prop: &str) -> Option<Self> {
        const CORNERS: [&str; 8] = [
            "top-left",
            "top-right",
            "bottom-left",
            "bottom-right",
            "start-start",
            "start-end",
            "end-start",
            "end-end",
        ];
        const SIDES: [&str; 15] = [
            "",
            "-top",
            "-right",
            "-bottom",
            "-left",
            "-inline",
            "-block",
            "-inline-start",
            "-inline-end",
            "-block-start",
            "-block-end",
            "-before",
            "-after",
            "-start",
            "-end",
        ];
        match unprefixed(prop) {
            "border-radius" | "rx" | "ry" => return Some(Family::Radius),
            "font-size" => return Some(Family::FontSize),
            "font-family" => return Some(Family::Face),
            "font" => return Some(Family::Font),
            "stroke-width" | "outline-width" | "text-decoration-thickness" => {
                return Some(Family::Width);
            }
            "outline" => return Some(Family::Border),
            _ => {}
        }
        let rest = unprefixed(prop).strip_prefix("border")?;
        if let Some(corner) = rest
            .strip_prefix('-')
            .and_then(|r| r.strip_suffix("-radius"))
        {
            return CORNERS.contains(&corner).then_some(Family::Radius);
        }
        match rest.strip_suffix("-width") {
            Some(side) if SIDES.contains(&side) => Some(Family::Width),
            Some(_) => None,
            None => SIDES.contains(&rest).then_some(Family::Border),
        }
    }

    /// Whether the token `name` belongs to this family.
    fn accepts(self, name: &str, tokens: &Tokens) -> bool {
        let starts = |prefixes: &[&str]| prefixes.iter().any(|p| name.starts_with(p));
        match self {
            Family::Radius => starts(&["--radius-"]),
            Family::FontSize => starts(&["--font-size-"]),
            Family::Face => starts(&["--font-sans", "--font-mono"]),
            Family::Font => starts(&[
                "--font-size-",
                "--font-sans",
                "--font-mono",
                "--font-weight-",
            ]),
            Family::Width => starts(&["--line-"]),
            Family::Border => starts(&["--line-"]) || tokens.colours.contains(name),
        }
    }

    /// Whether a bare word may stand in a value of this family besides its tokens.
    fn allows(self, word: &str) -> bool {
        let zero = matches!(word, "0" | "none");
        CSS_WIDE.contains(&word)
            || word == "!important"
            || match self {
                Family::Radius | Family::Width => zero,
                Family::Border => {
                    word == "0"
                        || BORDER_STYLES.contains(&word)
                        || matches!(word, "transparent" | "currentcolor")
                }
                _ => false,
            }
    }
}

/// `value` without its `var()` calls, and the custom properties those calls name (a fallback
/// stays in the rest, its own `var()` calls count as well).
fn split_vars(value: &str) -> (String, Vec<String>) {
    let lower = value.to_ascii_lowercase();
    let (mut rest, mut names, mut pos) = (String::new(), Vec::new(), 0);
    while let Some(i) = lower[pos..].find("var(").map(|n| pos + n) {
        rest.push_str(&value[pos..i]);
        rest.push(' ');
        let args_at = i + 4;
        let (mut depth, mut end) = (1, value.len());
        for (j, c) in value[args_at..].char_indices() {
            depth += i32::from(c == '(') - i32::from(c == ')');
            if depth == 0 {
                end = args_at + j;
                break;
            }
        }
        let (name, fallback) = value[args_at..end]
            .split_once(',')
            .unwrap_or((&value[args_at..end], ""));
        names.push(name.trim().to_string());
        let (inner, inner_names) = split_vars(fallback);
        rest.push_str(&inner);
        rest.push(' ');
        names.extend(inner_names);
        pos = (end + 1).min(value.len());
    }
    rest.push_str(&value[pos..]);
    (rest, names)
}

/// Rule 1 for a property that must hold tokens only: unknown tokens, tokens of another family
/// and literals. Colours are reported by `colour_hits`, not here.
fn token_findings(prop: &str, value: &str, at: usize, tokens: &Tokens, out: &mut Vec<Finding>) {
    let Some(family) = Family::of(prop) else {
        return;
    };
    let (rest, names) = split_vars(value);
    for name in names {
        if !tokens.all.contains(&name) {
            out.push((TOKEN_RULE, at, format!("`{name}` in `{prop}` is no token")));
        } else if !family.accepts(&name, tokens) {
            out.push((
                TOKEN_RULE,
                at,
                format!("`{name}` in `{prop}` is a token of another family"),
            ));
        }
    }
    let literal = rest
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .filter(|w| !family.allows(w) && colour_hits(w, true).is_empty())
        .collect::<Vec<_>>();
    if !literal.is_empty() {
        out.push((
            TOKEN_RULE,
            at,
            format!("literal `{}` in `{prop}`", literal.join(" ")),
        ));
    }
}

// ---------------------------------------------------------------------------------------------
// Islands
// ---------------------------------------------------------------------------------------------

/// Rule 1 for an island: its string literals are scanned for colours, its comments never.
fn scan_island(text: &str, out: &mut Vec<Finding>) {
    for (at, string) in lex(text, true).1 {
        for lit in island_colours(string) {
            out.push((
                TOKEN_RULE,
                at,
                format!("colour literal `{lit}` in a string"),
            ));
        }
    }
}

/// The colours a string literal holds. The string itself is a colour when it is exactly a hex
/// colour (`#rgb`, `#rrggbb`, `#rrggbbaa`; `#rgba` is too often an element id) or a colour word.
/// A colour function counts anywhere. A hex or a word counts inside a CSS-like string, one with
/// a `:`, a `;` or a border style, where it stands as a value.
fn island_colours(string: &str) -> Vec<String> {
    let word = string.trim();
    let exact_hex = word
        .strip_prefix('#')
        .is_some_and(|d| matches!(d.len(), 3 | 6 | 8) && d.chars().all(|c| c.is_ascii_hexdigit()));
    if exact_hex || is_named_colour(&word.to_ascii_lowercase()) {
        return vec![word.to_string()];
    }
    let css_like = string.contains([':', ';'])
        || string
            .split(|c: char| !is_word(c))
            .any(|w| BORDER_STYLES[2..].contains(&w));
    let values = if string.contains(':') {
        declarations(string, 0)
            .into_iter()
            .map(|(_, _, v)| v)
            .collect()
    } else {
        vec![string]
    };
    values
        .into_iter()
        .flat_map(|v| colour_hits(v, true))
        .map(|(_, lit)| lit)
        .filter(|lit| css_like || COLOUR_FUNCTIONS.contains(&lit.to_ascii_lowercase().as_str()))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Rule 2: component ownership
// ---------------------------------------------------------------------------------------------

/// The class of a component: its name with `_` turned into `-`.
fn class_of(name: &str) -> String {
    name.replace('_', "-")
}

/// Rule 2 for a `class` attribute: tokens split on whitespace and on template delimiters.
fn owned_classes(rel: &Path, classes: &str, at: usize, out: &mut Vec<Finding>) {
    for name in COMPONENTS {
        if rel == Path::new(&format!("templates/components/{name}.html")) {
            continue;
        }
        let root = class_of(name);
        let owned = classes
            .split(|c: char| c.is_whitespace() || matches!(c, '{' | '}' | '%'))
            .any(|t| {
                t == root
                    || t.starts_with(&format!("{root}-"))
                    || t.starts_with(&format!("{root}__"))
            });
        if owned {
            out.push((
                OWNERSHIP_RULE,
                at,
                format!("`{root}` belongs to the `{name}` macro"),
            ));
        }
    }
}

/// Rule 2 for comment-free CSS: a selector `.<root>`, `.<root>-x` or `.<root>__x` of a
/// component whose stylesheet is not `own`.
fn owned_selectors(css: &str, base: usize, own: impl Fn(&str) -> bool, out: &mut Vec<Finding>) {
    for name in COMPONENTS {
        if own(name) {
            continue;
        }
        let selector = format!(".{}", class_of(name));
        for (i, _) in css.match_indices(&selector) {
            let after = &css[i + selector.len()..];
            if !after.starts_with(is_word) || after.starts_with('-') || after.starts_with("__") {
                out.push((
                    OWNERSHIP_RULE,
                    base + i,
                    format!("`{selector}` belongs to static/{name}.css"),
                ));
            }
        }
    }
}
