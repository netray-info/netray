//! The entry, gone and privacy pages as complete HTML documents with one inlined stylesheet.

use std::sync::OnceLock;

use askama::Template;

use super::view::EntryPageView;
use crate::components::QueryInputView;
use crate::tokens;

const COMPONENT_CSS: [&str; 10] = [
    include_str!("../../static/grade.css"),
    include_str!("../../static/run_line.css"),
    include_str!("../../static/progress_ray.css"),
    include_str!("../../static/spectrum.css"),
    include_str!("../../static/trace.css"),
    include_str!("../../static/check_row.css"),
    include_str!("../../static/evidence_block.css"),
    include_str!("../../static/diff_block.css"),
    include_str!("../../static/grouping_switch.css"),
    include_str!("../../static/query_input.css"),
];

fn stylesheet() -> &'static str {
    static SHEET: OnceLock<String> = OnceLock::new();
    SHEET.get_or_init(|| {
        let mut parts = vec![tokens::CSS, include_str!("../../static/base.css")];
        parts.extend(COMPONENT_CSS);
        parts.join("\n")
    })
}

#[derive(Template)]
#[template(path = "pages/entry.html")]
struct EntryPage<'a> {
    styles: &'static str,
    view: &'a EntryPageView,
}

#[derive(Template)]
#[template(path = "pages/gone.html")]
struct GonePage {
    styles: &'static str,
    query: QueryInputView,
}

#[derive(Template)]
#[template(path = "pages/privacy.html")]
struct PrivacyPage {
    styles: &'static str,
}

/// The entry page of a stored entry.
pub fn entry(view: &EntryPageView) -> askama::Result<String> {
    EntryPage {
        styles: stylesheet(),
        view,
    }
    .render()
}

/// The page of an expired or unknown entry.
pub fn gone() -> askama::Result<String> {
    GonePage {
        styles: stylesheet(),
        query: QueryInputView {
            action: "/".to_string(),
            name: "d".to_string(),
            label: "Check a domain".to_string(),
            placeholder: "example.com".to_string(),
            value: None,
            submit: "Check".to_string(),
        },
    }
    .render()
}

/// The privacy page.
pub fn privacy() -> askama::Result<String> {
    PrivacyPage {
        styles: stylesheet(),
    }
    .render()
}
