//! Contract tests for evidence diffing (spec v2-history, Phase 1, requirement 6: C5, C18, C19).
use netray_model::diff::{BlockChange, BlockChangeKind, diff};
use netray_model::evidence::{Block, BlockId, Node, Record, Shape, Value};
use netray_model::run::{Run, RunSection, SectionKind};
use netray_model::{Grade, Protocol};

fn id(s: &str) -> BlockId {
    BlockId::parse(s).expect("valid block id")
}

fn t(s: &str) -> Value {
    Value::Text(s.into())
}

fn block(name: &str, shape: Shape) -> Block {
    Block {
        id: id(name),
        title: "t".into(),
        shape,
    }
}

fn run(protocol: Protocol, outcome: SectionKind, blocks: Vec<Block>) -> Run {
    Run {
        profile_version: 1,
        engine_version: "0.26.0".into(),
        evidence_schema_version: 1,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol,
            outcome,
            reason: None,
        }],
        checks: vec![],
        blocks,
    }
}

fn measured(protocol: Protocol, blocks: Vec<Block>) -> Run {
    run(protocol, SectionKind::Measured, blocks)
}

fn table(rows: Vec<Vec<Value>>) -> Shape {
    Shape::Table {
        key: "name".into(),
        columns: vec!["name".into(), "value".into()],
        rows,
    }
}

fn record(data: &str) -> Record {
    Record {
        owner: t("example.com"),
        rtype: t("A"),
        ttl: Value::Duration(300),
        data: t(data),
    }
}

fn tree(leaf: &str) -> Shape {
    Shape::Tree(Node {
        fields: vec![("name".into(), t("root"))],
        children: vec![Node {
            fields: vec![("name".into(), t(leaf))],
            children: vec![],
        }],
    })
}

fn chain(second: &str) -> Shape {
    Shape::Chain(vec![
        vec![("subject".into(), t("leaf.example.com"))],
        vec![("subject".into(), t(second))],
    ])
}

fn changed(block: &str, items: &[&str]) -> BlockChange {
    BlockChange {
        block: id(block),
        kind: BlockChangeKind::Changed {
            items: items.iter().map(|s| s.to_string()).collect(),
        },
    }
}

/// Asserts exactly one `Changed` on `name` with `n` items whose format the engine chooses.
fn assert_one_changed(prev: Shape, cur: Shape, protocol: Protocol, name: &str, n: usize) {
    let p = measured(protocol, vec![block(name, prev)]);
    let c = measured(protocol, vec![block(name, cur)]);
    let ev = diff(&p, &c, &[], None).evidence;
    assert_eq!(ev.len(), 1, "{name}: {ev:?}");
    assert_eq!(ev[0].block, id(name));
    match &ev[0].kind {
        BlockChangeKind::Changed { items } => assert_eq!(items.len(), n, "{name}: {items:?}"),
        other => panic!("{name}: expected Changed, got {other:?}"),
    }
}

#[test]
fn evidence_diff_per_shape_with_deterministic_items() {
    let rows: Vec<(&str, Protocol, Shape, Shape, Vec<BlockChange>)> = vec![
        (
            "tls.handshake@192.0.2.1:443",
            Protocol::Tls,
            Shape::Fields(vec![("a".into(), t("1")), ("b".into(), t("2"))]),
            Shape::Fields(vec![("a".into(), t("1")), ("b".into(), t("3"))]),
            vec![changed("tls.handshake@192.0.2.1:443", &["~ b"])],
        ),
        (
            "http.headers",
            Protocol::Http,
            table(vec![
                vec![t("a"), t("1")],
                vec![t("b"), t("2")],
                vec![t("c"), t("3")],
            ]),
            table(vec![
                vec![t("a"), t("1")],
                vec![t("b"), t("9")],
                vec![t("d"), t("4")],
            ]),
            vec![changed("http.headers", &["~ b", "- c", "+ d"])],
        ),
        (
            "tls.chain@192.0.2.1:443",
            Protocol::Tls,
            chain("ca.example.com"),
            chain("other-ca.example.com"),
            vec![changed("tls.chain@192.0.2.1:443", &["1"])],
        ),
        (
            "http.body",
            Protocol::Http,
            Shape::Text("one".into()),
            Shape::Text("two".into()),
            vec![changed("http.body", &[])],
        ),
    ];
    for (name, protocol, prev, cur, expected) in rows {
        let p = measured(protocol, vec![block(name, prev)]);
        let c = measured(protocol, vec![block(name, cur)]);
        assert_eq!(diff(&p, &c, &[], None).evidence, expected, "{name}");
    }

    // records: an added record is one descriptor (format is the engine's)
    assert_one_changed(
        Shape::Records(vec![record("192.0.2.1")]),
        Shape::Records(vec![record("192.0.2.1"), record("192.0.2.2")]),
        Protocol::Dns,
        "dns.a",
        1,
    );

    // tree: one changed node is one path (format is the engine's)
    assert_one_changed(
        tree("x.example.com"),
        tree("y.example.com"),
        Protocol::Tls,
        "tls.tree",
        1,
    );
}

#[test]
fn evidence_diff_block_only_in_one_run_is_added_or_removed() {
    let with = measured(
        Protocol::Http,
        vec![block("http.body", Shape::Text("x".into()))],
    );
    let without = measured(Protocol::Http, vec![]);
    assert_eq!(
        diff(&without, &with, &[], None).evidence,
        vec![BlockChange {
            block: id("http.body"),
            kind: BlockChangeKind::Added
        }]
    );
    assert_eq!(
        diff(&with, &without, &[], None).evidence,
        vec![BlockChange {
            block: id("http.body"),
            kind: BlockChangeKind::Removed
        }]
    );
}

#[test]
fn evidence_diff_skips_sections_not_measured_in_both_runs() {
    let p = measured(
        Protocol::Http,
        vec![block("http.body", Shape::Text("one".into()))],
    );
    let c = run(
        Protocol::Http,
        SectionKind::TimedOut,
        vec![block("http.body", Shape::Text("two".into()))],
    );
    assert_eq!(diff(&p, &c, &[], None).evidence, vec![]);
}

#[test]
fn table_row_move_is_not_a_change_only_the_changed_row_is_reported() {
    let prev = measured(
        Protocol::Http,
        vec![block(
            "http.headers",
            table(vec![
                vec![t("a"), t("1")],
                vec![t("b"), t("2")],
                vec![t("c"), t("3")],
            ]),
        )],
    );
    // b changed, c moved to the front
    let cur = measured(
        Protocol::Http,
        vec![block(
            "http.headers",
            table(vec![
                vec![t("c"), t("3")],
                vec![t("a"), t("1")],
                vec![t("b"), t("9")],
            ]),
        )],
    );
    assert_eq!(
        diff(&prev, &cur, &[], None).evidence,
        vec![changed("http.headers", &["~ b"])]
    );
}

#[test]
fn volatile_paths_suppress_evidence_changes() {
    let headers = |date: &str| {
        measured(
            Protocol::Http,
            vec![block(
                "http.headers",
                table(vec![
                    vec![t("date"), t(date)],
                    vec![t("server"), t("example")],
                ]),
            )],
        )
    };
    let prev = headers("Mon, 01 Jan 2026 00:00:00 GMT");
    let cur = headers("Tue, 02 Jan 2026 00:00:00 GMT");
    assert_eq!(
        diff(&prev, &cur, &["http.headers/value@date"], None).evidence,
        vec![]
    );
    // without the volatile list the same pair is a change
    assert_eq!(
        diff(&prev, &cur, &[], None).evidence,
        vec![changed("http.headers", &["~ date"])]
    );

    let hs = |ms| {
        measured(
            Protocol::Tls,
            vec![block(
                "tls.handshake@192.0.2.1:443",
                Shape::Fields(vec![("handshake_ms".into(), Value::Duration(ms))]),
            )],
        )
    };
    assert_eq!(
        diff(&hs(10), &hs(55), &["tls.handshake/handshake_ms"], None).evidence,
        vec![]
    );
}
