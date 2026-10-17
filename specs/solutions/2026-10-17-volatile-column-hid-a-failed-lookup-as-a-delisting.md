---
class: guard-misclassifies-lookalike-input
repeat-of: specs/solutions/2026-10-15-deny-list-flip-refused-transition-shorthand.md
---
# A volatile `answered` column made a rate-limited DNSBL zone read as a delisting

**What failed.** To keep failed domain-zone lookups from storing history entries, `email.dnsbl_domain/answered` was made volatile (ae9d035), then the `answer` of an unanswered row blanked (55777a8). The branch review showed a policy code still reached the compared `answer` column, then that the blank was a placeholder and that listed → rate-limited still stored an entry reading as a delisting: no stateless projection can tell a failed lookup from a cleared listing (review e4d38be..55777a8).
**What worked.** Compare `email.dnsbl_domain` whole with codes as returned and record every transition (83fc15e), held by `a_policy_answer_from_the_domain_zone_keeps_its_code_and_is_recorded` and `a_failed_domain_zone_lookup_is_recorded`; the cost (rate-limit flaps store entries) is named in the decision log.
**How to notice next time.** When a volatile path is meant to hide a lookup failure, ask what the row looks like after listed → failed: if it equals cleared, the projection lies.
