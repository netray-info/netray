# Intent: fix-ip-evidence-race

## Problem

CI on main (run 38523840435) fails `netray-ip --test evidence`, `listed_address_row_carries_the_matching_flag`: `c2` false where true is expected. Both tests in that binary build their list file under `temp_dir()/netray-ip-evidence-{pid}-feodo_botnet_ips` and remove it when done; run in parallel, one test deletes the directory while the other's module is still loading the list.

## Proposed outcome

Each `run` call uses its own directory; the test passes under parallel execution.

## Affected users and systems

The netray-ip test suite only; no production code.

## Constraints

Blocks the 0.26.0 push.

## Open questions

None.
