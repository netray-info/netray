//! C19: `tls.strong_signature` states what it flags and that root self-signatures are ignored.

use netray_engine::catalogue;
use netray_model::CheckId;

#[test]
fn strong_signature_finding_names_scope_and_ignores_root_self_signatures() {
    let cat = catalogue();
    let e = cat
        .get(&CheckId::parse("tls.strong_signature").unwrap())
        .expect("tls.strong_signature in catalogue");
    assert_eq!(
        e.finding,
        "The leaf or an intermediate certificate is signed with SHA-1 or MD5, or with an algorithm netray does not recognise; root self-signatures are ignored."
    );
    assert_eq!(
        e.fix_hint,
        "Replace the affected certificates with ones signed using SHA-256 or stronger."
    );
}
