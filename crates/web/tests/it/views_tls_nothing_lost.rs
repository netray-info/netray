//! v2-tls-ui Phase 1 (C2, C25): the TLS port view takes a block only when it renders every
//! value of it; a port of two addresses with every registered key shows each value's display
//! text in the addresses fragment, and the blocks it took render an empty fragment.
use std::collections::{BTreeMap, BTreeSet};

use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_model::{CheckId, Status};
use netray_web::evidence::display_value;
use netray_web::links::decorate;
use netray_web::pages::views::{ViewContext, render_with};

const V4: &str = "192.0.2.1";
const V6: &str = "2001:db8::1";

/// askama 0.16's HTML escaping of a text value.
fn escaped(text: &str) -> String {
    text.replace('&', "&#38;")
        .replace('<', "&#60;")
        .replace('>', "&#62;")
        .replace('"', "&#34;")
        .replace('\'', "&#39;")
}

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn addr(s: &str) -> Value {
    Value::Addr(s.parse().unwrap())
}

/// RFC 3339 UTC time of a unix second count, and its date.
fn rfc3339(secs: i64) -> (String, String) {
    // Civil-from-days (Howard Hinnant), independent of the code under test.
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let date = format!("{y:04}-{m:02}-{d:02}");
    let time = format!(
        "{date}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    );
    (time, date)
}

/// The fixture blocks, the texts that must appear in the addresses fragment, and the keys held.
#[derive(Default)]
struct Fixture {
    blocks: Vec<Block>,
    needles: Vec<String>,
    keys: BTreeMap<String, BTreeSet<String>>,
}

impl Fixture {
    fn note_key(&mut self, id: &str, key: &str) {
        let base = id.split('@').next().unwrap().to_string();
        self.keys.entry(base).or_default().insert(key.to_string());
    }

    /// A pair whose display text, escaped, must appear.
    fn pair(&mut self, id: &str, key: &str, value: Value) -> (String, Value) {
        self.needles.push(escaped(&display_value(&value)));
        self.note_key(id, key);
        (key.to_string(), value)
    }

    /// A pair the view shows in its own words: `needle` must appear instead.
    fn shown(&mut self, id: &str, key: &str, value: Value, needle: &str) -> (String, Value) {
        self.needles.push(escaped(needle));
        self.note_key(id, key);
        (key.to_string(), value)
    }

    /// A pair read without a needle (absence of a chip, row shape, order, note).
    fn read(&mut self, id: &str, key: &str, value: Value) -> (String, Value) {
        self.note_key(id, key);
        (key.to_string(), value)
    }

    fn push(&mut self, id: &str, shape: Shape) {
        self.blocks.push(Block {
            id: BlockId::parse(id).unwrap(),
            title: id.into(),
            shape,
        });
    }
}

fn chain(f: &mut Fixture, id: &str, now: i64) -> Shape {
    // Whole days with an hour of slack, so the days left the view computes equals the stored.
    let leaf_before = rfc3339(now - 10 * 86_400).1;
    let leaf_after = rfc3339(now + 50 * 86_400 + 3_600).1;
    let ca_before = rfc3339(now - 300 * 86_400).1;
    let ca_after = rfc3339(now + 1_234 * 86_400 + 3_600).1;
    let leaf = vec![
        f.read(id, "position", text("leaf")),
        f.pair(id, "subject", text("CN=example.com, O=Example & Sons")),
        f.pair(id, "issuer", text("CN=Example Issuing CA 1")),
        f.pair(id, "sans", text("example.com")),
        f.pair(id, "sans", text("www.example.com")),
        f.shown(
            id,
            "not_before",
            Value::Time(rfc3339(now - 10 * 86_400).0),
            &leaf_before,
        ),
        f.shown(
            id,
            "not_after",
            Value::Time(rfc3339(now + 50 * 86_400 + 3_600).0),
            &leaf_after,
        ),
        f.shown(id, "days_remaining", Value::Int(50), "50 days left"),
        f.pair(id, "key", text("ECDSA P-256")),
        f.pair(id, "signature_algorithm", text("ecdsa-with-SHA256")),
        f.pair(
            id,
            "fingerprint_sha256",
            text("4F:9A:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:E1"),
        ),
        f.pair(
            id,
            "fingerprint_sha1",
            text("A1:B2:C3:D4:E5:F6:07:18:29:3A:4B:5C:6D:7E:8F:90:A1:B2:C3:D4"),
        ),
        f.pair(id, "serial", text("0A1B2C3D4E5F")),
        f.pair(id, "policy", text("OV")),
        f.read(id, "self_signed", Value::Bool(false)),
        f.read(id, "expired", Value::Bool(false)),
        f.pair(id, "ocsp_url", text("http://ocsp.example.com")),
        f.pair(id, "ca_issuers_url", text("http://ca.example.com/issuer.crt")),
    ];
    let ca = vec![
        f.read(id, "position", text("intermediate")),
        f.pair(id, "subject", text("CN=Example Issuing CA 1")),
        f.pair(id, "issuer", text("CN=Example Root CA")),
        f.pair(id, "sans", text("ca.example.com")),
        f.shown(
            id,
            "not_before",
            Value::Time(rfc3339(now - 300 * 86_400).0),
            &ca_before,
        ),
        f.shown(
            id,
            "not_after",
            Value::Time(rfc3339(now + 1_234 * 86_400 + 3_600).0),
            &ca_after,
        ),
        f.shown(id, "days_remaining", Value::Int(1_234), "1234"),
        f.pair(id, "key", text("RSA 4096")),
        f.pair(id, "signature_algorithm", text("sha384WithRSAEncryption")),
        f.pair(
            id,
            "fingerprint_sha256",
            text("C0:DE:01:02:03:04:05:06:07:08:09:0A:0B:0C:0D:0E:0F:10:11:12:13:14:15:16:17:18:19:1A:1B:1C:1D:1E"),
        ),
        f.pair(
            id,
            "fingerprint_sha1",
            text("11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44"),
        ),
        f.pair(id, "serial", text("FEDCBA987654")),
        f.pair(id, "policy", text("DV")),
        f.read(id, "self_signed", Value::Bool(false)),
        f.read(id, "expired", Value::Bool(false)),
        f.pair(id, "ocsp_url", text("http://ocsp.ca.example.com")),
        f.pair(id, "ca_issuers_url", text("http://root.example.com/root.crt")),
    ];
    Shape::Chain(vec![leaf, ca])
}

fn handshake(f: &mut Fixture, id: &str, second: bool) -> Shape {
    let mut fields = vec![
        f.pair(id, "version", text("TLSv1.3")),
        f.pair(id, "cipher_suite", text("TLS_AES_256_GCM_SHA384")),
        f.pair(id, "key_exchange_group", text("X25519")),
        f.pair(id, "sni", text("example.com")),
        f.pair(id, "starttls", text("smtp")),
        f.shown(id, "ocsp_stapled", Value::Bool(true), "stapled"),
        f.pair(id, "ocsp_status", text("good")),
        f.shown(
            id,
            "ocsp_this_update",
            Value::Time("2026-10-08T00:00:00Z".into()),
            "2026-10-08",
        ),
        f.shown(
            id,
            "ocsp_next_update",
            Value::Time("2026-10-15T00:00:00Z".into()),
            "2026-10-15",
        ),
        f.pair(id, "ech", Value::Bool(true)),
    ];
    if second {
        fields.extend([
            f.pair(id, "alpn", text("http/1.1")),
            f.shown(
                id,
                "ocsp_live_status",
                text("revoked"),
                "live: revoked 2026-10-01 · keyCompromise",
            ),
            f.read(
                id,
                "ocsp_live_revoked_at",
                Value::Time("2026-10-01T00:00:00Z".into()),
            ),
            f.read(id, "ocsp_live_reason", text("keyCompromise")),
            f.shown(
                id,
                "handshake_ms",
                Value::Duration(31),
                "31 ms (2001:db8::1)",
            ),
        ]);
    } else {
        fields.extend([
            f.pair(id, "alpn", text("h2")),
            f.shown(id, "ocsp_live_status", text("good"), "live: good"),
            f.shown(id, "handshake_ms", Value::Duration(22), "22 ms (192.0.2.1)"),
        ]);
    }
    Shape::Fields(fields)
}

fn fixture() -> Fixture {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let mut f = Fixture::default();
    let a = "tls.addresses";
    let columns: Vec<String> = ["address", "port", "outcome", "message"]
        .map(String::from)
        .into();
    for c in &columns {
        f.note_key(a, c);
    }
    f.needles
        .extend([V4.to_string(), V6.to_string(), "Port 443".to_string()]);
    f.push(
        a,
        Shape::Table {
            key: "address".into(),
            columns,
            rows: vec![
                vec![addr(V4), Value::Int(443), text("ok"), text("")],
                vec![addr(V6), Value::Int(443), text("ok"), text("")],
            ],
        },
    );
    for (ip, second) in [(V4, false), (V6, true)] {
        let qualifier = if second {
            format!("[{ip}]:443")
        } else {
            format!("{ip}:443")
        };
        let id = format!("tls.chain@{qualifier}");
        let shape = chain(&mut f, &id, now);
        f.push(&id, shape);
        let id = format!("tls.handshake@{qualifier}");
        let shape = handshake(&mut f, &id, second);
        f.push(&id, shape);
    }
    let ct = "tls.ct";
    let columns: Vec<String> = ["address", "sct_count", "log_ids"].map(String::from).into();
    for c in &columns {
        f.note_key(ct, c);
    }
    f.needles.extend(
        [
            "3 SCTs",
            "2 SCTs",
            "log-a1, log-a2, log-a3",
            "log-b1, log-b2",
        ]
        .map(String::from),
    );
    f.push(
        ct,
        Shape::Table {
            key: "address".into(),
            columns,
            rows: vec![
                vec![addr(V4), Value::Int(3), text("log-a1, log-a2, log-a3")],
                vec![addr(V6), Value::Int(2), text("log-b1, log-b2")],
            ],
        },
    );
    f
}

#[test]
fn c2_c25_port_view_renders_every_value_of_the_blocks_it_takes() {
    let f = fixture();
    let statuses: Vec<(CheckId, Status)> = vec![];
    let decorate = decorate(&[]);
    let feeds = vec![];
    let ctx = ViewContext {
        statuses: &statuses,
        decorate: &decorate,
        feeds: &feeds,
    };

    let head = &f.blocks[0];
    let html = render_with(head, &ctx, &f.blocks)
        .expect("the port view must take tls.addresses of a port with every registered key");
    for needle in &f.needles {
        assert!(
            html.contains(needle.as_str()),
            "{needle:?} missing in: {html}"
        );
    }
    // The distinct-values rule: each differing value shows with its address.
    for (value, address) in [("h2", V4), ("http/1.1", V6)] {
        let needle = format!("{value} ({address})");
        assert!(html.contains(&needle), "{needle:?} missing in: {html}");
    }

    for block in &f.blocks {
        let id = block.id.to_string();
        let anchor = format!("id=\"ev-{id}\"");
        let count = html.matches(&anchor).count();
        if id.starts_with("tls.addresses") {
            assert_eq!(count, 1, "{anchor} must appear once in: {html}");
            continue;
        }
        assert_eq!(count, 1, "{anchor} must appear once in: {html}");
        assert_eq!(
            render_with(block, &ctx, &f.blocks).as_deref(),
            Some(""),
            "{id}: a taken block renders an empty fragment"
        );
    }

    // The fixture holds every key the register lists for these blocks.
    let register = &netray_tls::module::REGISTER;
    for block in ["tls.addresses", "tls.chain", "tls.handshake", "tls.ct"] {
        let registered: BTreeSet<String> = register
            .keys
            .iter()
            .filter(|(b, _, _)| *b == block)
            .map(|(_, key, _)| key.to_string())
            .collect();
        assert_eq!(
            f.keys.get(block).cloned().unwrap_or_default(),
            registered,
            "{block}: fixtures must hold every registered key"
        );
    }
}
