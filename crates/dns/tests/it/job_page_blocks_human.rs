//! v2-dns-ui Phase 1 (C14, block half): a TXT record node of `dns.answers` carries the
//! `human` text of `record_format::format_txt_human`.
use std::net::IpAddr;

use mhost::resources::rdata::TXT;
use netray_dns::config::Config;
use netray_dns::record_format::format_txt_human;
use netray_dns::security::RateLimitState;
use netray_dns::{query_page_blocks, testing};
use netray_model::evidence::{Node, Shape, Value};

fn text_of(node: &Node, key: &str) -> Option<String> {
    node.fields.iter().find_map(|(k, v)| match v {
        Value::Text(t) if k.to_string() == key => Some(t.clone()),
        _ => None,
    })
}

fn collect_txt<'a>(node: &'a Node, out: &mut Vec<&'a Node>) {
    if text_of(node, "type").as_deref() == Some("TXT") && text_of(node, "data").is_some() {
        out.push(node);
    }
    for child in &node.children {
        collect_txt(child, out);
    }
}

#[tokio::test]
async fn txt_record_node_carries_the_human_text() {
    let client: IpAddr = "203.0.113.7".parse().unwrap();
    let limiter = RateLimitState::new(&Config::default().limits);
    let parsed = netray_dns::parser::parse("example.com TXT").expect("query parses");
    let result = testing::stub_jobs()
        .query_parsed(parsed, client, &limiter, "rid".into())
        .await
        .expect("job starts")
        .collect()
        .await;
    let blocks = query_page_blocks(&result);
    let answers = blocks
        .iter()
        .find(|b| b.id.module() == "dns" && b.id.name() == "answers")
        .expect("dns.answers block");
    let Shape::Tree(root) = &answers.shape else {
        panic!("dns.answers is a tree");
    };
    let mut txts = vec![];
    collect_txt(root, &mut txts);
    assert!(!txts.is_empty(), "a TXT record node exists");
    let expected = format_txt_human(&TXT::new(vec!["v=spf1 -all".to_string()]));
    for node in txts {
        assert_eq!(text_of(node, "data").as_deref(), Some("\"v=spf1 -all\""));
        assert_eq!(text_of(node, "human"), Some(expected.clone()));
    }
}
