/// C19: the OG card timestamp formatter lives in netray-web and lens re-exports it
/// at its old path, so both answer the same string for the same instant.
use std::time::{Duration, SystemTime};

#[test]
fn og_timestamp_moved_both_paths_format_identically() {
    // (unix seconds, expected string)
    let cases: [(u64, &str); 3] = [
        (0, "Jan 1, 1970 \u{00b7} 00:00 UTC"),
        // 2028-02-29 23:59:00 UTC
        (1_835_481_540, "Feb 29, 2028 \u{00b7} 23:59 UTC"),
        // 2026-05-08 17:44:00 UTC (ts() of og_render.rs)
        (1_778_262_240, "May 8, 2026 \u{00b7} 17:44 UTC"),
    ];
    for (secs, expected) in cases {
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(secs);
        assert_eq!(
            netray_web::entries::view::format_utc_timestamp(t),
            expected,
            "netray_web at {secs}"
        );
        assert_eq!(
            lens::og::render::format_utc_timestamp(t),
            expected,
            "lens at {secs}"
        );
    }
}
