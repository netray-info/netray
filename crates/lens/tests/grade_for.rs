//! `lens::score::grade_for` maps a percentage onto the profile's thresholds at their boundaries.

use lens::score::grade_for;
use netray_engine::profile::{Profile, Profiles};

#[test]
fn current_profile_threshold_boundaries() {
    let p = Profiles::current();
    for (pct, expected) in [
        (100.0, "A+"),
        (97.0, "A+"),
        (96.9, "A"),
        (90.0, "A"),
        (89.9, "B"),
        (75.0, "B"),
        (74.9, "C"),
        (60.0, "C"),
        (59.9, "D"),
        (40.0, "D"),
        (39.9, "F"),
        (0.0, "F"),
    ] {
        assert_eq!(grade_for(p, pct), expected, "{pct}%");
    }
}

#[test]
fn below_every_threshold_is_f() {
    let p = Profile::from_toml(
        "[meta]\nname = \"test\"\nversion = 3\n\n\
         [sections.dns]\nweight = 100\n\n[sections.dns.checks]\ndnssec = 5\n\n\
         [thresholds]\n\"A\" = 90\n\"B\" = 75\n",
    )
    .unwrap();
    assert_eq!(grade_for(&p, 75.0), "B");
    assert_eq!(grade_for(&p, 74.9), "F");
    assert_eq!(grade_for(&p, 0.0), "F");
}
