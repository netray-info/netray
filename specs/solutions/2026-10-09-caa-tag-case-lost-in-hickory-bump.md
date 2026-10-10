---
class: guard-misclassifies-lookalike-input
repeat-of: none
---
# A dependency bump let an uppercase CAA tag pass tlsight's CAA check

**What failed.** hickory-proto 0.25 lowercased the known CAA tags (`Property::from`, `caa.rs:259`); 0.26.3 keeps the wire case (`read_tag`, `caa.rs:565`), and mhost 0.12.0 copies it. tlsight compared `r.tag == "issue"` (`crates/tls/src/dns/caa.rs:29`), so `CAA 0 ISSUE "digicert.com"` produced no issue domain and `caa_compliant` returned Pass for a Let's Encrypt leaf. The phase review of the bump did not see it; the feature review did.
**What worked.** A test with `ISSUE`/`IssueWild` tags (`caa_tags_match_case_insensitively`), then `caa_record` lowercases the tag when the record is built and both filters compare with `eq_ignore_ascii_case` (`1ae32db`). tlsight's API output keeps lowercase tags.
**How to notice next time.** A parser upgrade whose changelog or diff drops a normalisation step, while our code compares the parsed value with a literal.
