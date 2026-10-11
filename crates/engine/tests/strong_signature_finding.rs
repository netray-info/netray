//! C19, C21: the catalogue findings of `tls.strong_signature` and `http.cors` name every
//! branch their checks fail or warn on.

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
        "The leaf or an intermediate certificate is signed with SHA-1, MD5 or MD2, or with an algorithm netray does not recognise; root self-signatures are ignored."
    );
    assert_eq!(
        e.fix_hint,
        "Replace the affected certificates with ones signed using SHA-256 or stronger."
    );
}

#[test]
fn cors_finding_names_the_null_origin() {
    let cat = catalogue();
    let e = cat
        .get(&CheckId::parse("http.cors").unwrap())
        .expect("http.cors in catalogue");
    assert_eq!(
        e.finding,
        "The site echoes an untrusted request Origin in Access-Control-Allow-Origin, or answers with Access-Control-Allow-Origin: null, together with Access-Control-Allow-Credentials: true, or allows any origin with Access-Control-Allow-Origin: *."
    );
}
