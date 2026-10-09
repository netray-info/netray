# Plan: raw query policy

## Phase 1 — Outbound context

### Groups

- G1: C1–C20. One group: every test is a `#[cfg(test)]` module of package `prism`, so nothing compiles until every item of the context exists, and C2, C6 and C20 span several files. Inside G1, `dns_raw.rs` comes first; the four callers are independent of each other.

### Plan

Paths under `crates/mhost-prism/src/`.

1. `dns_raw.rs` (C1, C19): add `SendFuture`, `trait RawSend`, `NOT_PUBLIC`, `ServerList`, `#[derive(Clone)] RawOutbound { allow, resolver, sender }` and a private `UdpTcpSender` (today's `raw_query`/`raw_query_dnssec` merged into one UDP-with-TCP-fallback query). `RawOutbound::production()` = `is_allowed_target`, `SystemResolver`, `UdpTcpSender` (prior art `Outbound::production`, `api/check.rs`). Methods `parallel_queries`, `raw_query_dnssec` (through the sender, no policy check, so the root servers pass), `resolve_missing_glue` (through the resolver, only names with no glue; refused glue counts as present; IPv4 kept) and `build_server_list -> ServerList` (wraps the free `build_server_list` with `ipv4 && allow`, counts refused IPv4). The free `parallel_queries`, `raw_query`, `raw_query_dnssec`, `resolve_missing_glue` go away; `build_server_list`, `build_query`, `send_udp`, `send_tcp` stay.
2. `api/check.rs` (C3, C7–C11, C18): `check_ns_lame_delegation` and `check_ns_delegation_consistency` take `raw: &RawOutbound`; server list from `raw.build_server_list`; all refused → only `Warning(NOT_PUBLIC)`; some refused → the warning once plus today's lines; the call site builds `RawOutbound::production()` beside `Outbound::production()` and passes it into the `tokio::join!`.
3. `api/authcompare.rs` (C5, C12, C13): `AuthServers`, `resolve_auth_servers` (replacing the inline resolution in `post_handler`, labels from the allowed list, `NOT_PUBLIC` once in warnings) and `query_auth_servers` (the auth branch's fan-out); the handler extends `all_warnings` with `auth.warnings`.
4. `dns_dnssec.rs` (C4, C14–C16, C20): `walk_chain_with(raw, …)`; `walk_chain` calls it with the production context; `query_record_type_dnssec` takes `raw`; the empty-server check emits a level with `warning NOT_PUBLIC` and stops when the list is empty because of refusals.
5. `dns_trace.rs` (C17, C20): `walk_with(raw, …)`; `walk` calls it with the production context; its own filter goes, the context decides (`check_target_ip` is the negation of `is_blocked_ip`, the same set as `is_allowed_target`, so output stays the same).
