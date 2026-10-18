//! Explore-live links for the HTTP and IP views (V2 spec v2-inspectors, requirement 8, Phase 3):
//! http blocks yield none, ip.addresses rows link to the IP inspector, query values are encoded.
use netray_model::Protocol;
use netray_model::evidence::{Block, BlockId, Record, Shape, Value};
use netray_web::explore::{block_links, view_link, view_links};

fn block(id: &str, shape: Shape) -> Block {
    Block {
        id: BlockId::parse(id).unwrap(),
        title: id.to_string(),
        shape,
    }
}

fn hrefs(links: &[netray_web::explore::ExploreLink]) -> Vec<&str> {
    links.iter().map(|l| l.href.as_str()).collect()
}

#[test]
fn http_view_links_only_itself_and_headers_yield_none() {
    let headers = block(
        "http.headers",
        Shape::Fields(vec![("server".into(), Value::Text("example".into()))]),
    );
    let redirects = block("http.redirects", Shape::Chain(vec![]));
    assert!(block_links(&headers, "example.com").is_empty());
    let view = view_link(Protocol::Http, "example.com");
    assert_eq!(view.href, "/inspect/http?url=https%3A%2F%2Fexample.com%2F");
    let all = view_links(Protocol::Http, "example.com", &[headers, redirects]);
    assert_eq!(all, vec![view]);
}

#[test]
fn ip_addresses_rows_link_to_the_ip_inspector() {
    let table = block(
        "ip.addresses",
        Shape::Table {
            key: "address".into(),
            columns: vec!["address".into()],
            rows: vec![
                vec![Value::Addr("192.0.2.7".parse().unwrap())],
                vec![Value::Addr("2001:db8::7".parse().unwrap())],
            ],
        },
    );
    assert_eq!(
        hrefs(&block_links(&table, "example.com")),
        vec!["/inspect/ip?ip=192.0.2.7", "/inspect/ip?ip=2001:db8::7"]
    );
}

#[test]
fn query_values_are_percent_encoded() {
    let txt = block(
        "dns.txt",
        Shape::Records(vec![Record {
            owner: Value::Text("a&b.example.com".into()),
            rtype: Value::Text("TXT".into()),
            ttl: Value::Int(300),
            data: Value::Text("v=x".into()),
        }]),
    );
    let links = block_links(&txt, "example.com");
    assert_eq!(
        hrefs(&links),
        vec!["/inspect/dns?q=a%26b.example.com%20TXT"]
    );
    let query = links[0].href.split_once('?').unwrap().1;
    assert!(!query.contains([' ', '[', ']', '&']));
}
