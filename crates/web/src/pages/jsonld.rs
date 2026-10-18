//! The structured data the guides embed as `application/ld+json` blocks.

use serde::Serialize;

const CONTEXT: &str = "https://schema.org";

#[derive(Serialize)]
pub(crate) struct Organization {
    #[serde(rename = "@type")]
    kind: &'static str,
    name: &'static str,
    url: &'static str,
}

#[derive(Serialize)]
pub(crate) struct Article {
    #[serde(rename = "@context")]
    context: &'static str,
    #[serde(rename = "@type")]
    kind: &'static str,
    headline: String,
    description: String,
    url: String,
    publisher: Organization,
    #[serde(rename = "mainEntityOfPage")]
    main_entity_of_page: String,
}

impl Article {
    pub(crate) fn new(headline: &str, description: &str, url: &str) -> Self {
        Self {
            context: CONTEXT,
            kind: "Article",
            headline: headline.to_string(),
            description: description.to_string(),
            url: url.to_string(),
            publisher: Organization {
                kind: "Organization",
                name: "netray.info",
                url: "https://netray.info",
            },
            main_entity_of_page: url.to_string(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct ListItem {
    #[serde(rename = "@type")]
    kind: &'static str,
    position: u32,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    item: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct BreadcrumbList {
    #[serde(rename = "@context")]
    context: &'static str,
    #[serde(rename = "@type")]
    kind: &'static str,
    #[serde(rename = "itemListElement")]
    item_list_element: Vec<ListItem>,
}

impl BreadcrumbList {
    /// `(name, item)` per crumb, positions from 1.
    pub(crate) fn new(crumbs: &[(&str, Option<&str>)]) -> Self {
        Self {
            context: CONTEXT,
            kind: "BreadcrumbList",
            item_list_element: crumbs
                .iter()
                .zip(1..)
                .map(|((name, item), position)| ListItem {
                    kind: "ListItem",
                    position,
                    name: name.to_string(),
                    item: item.map(str::to_string),
                })
                .collect(),
        }
    }
}

/// The JSON of `value` with `<`, `>` and `&` escaped, safe inside a `<script>` element.
pub(crate) fn script_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .expect("structured data serialises")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
}
