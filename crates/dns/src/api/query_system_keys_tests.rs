// v2-correctness Phase 4 (R9): the resolver group of a query carries one breaker key per
// resolver, whatever the number of system nameservers. With one system nameserver this holds
// already; with two or more it fails until `@system` pushes one key per resolver.

use std::time::Duration;

use super::build_resolver_group;
use super::build_resolver_group_tests::{config, parsed};
use crate::parser::ServerSpec;

#[tokio::test]
async fn the_system_group_holds_one_breaker_key_per_resolver() {
    let q = parsed(vec![ServerSpec::System]);
    let (group, keys) = build_resolver_group(&q, &config(true), Duration::from_secs(1))
        .await
        .expect("system group builds");
    assert_eq!(
        keys.len(),
        group.resolvers().len(),
        "keys {keys:?} for {} resolvers",
        group.resolvers().len()
    );
}
