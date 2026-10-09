// Fixture for tests/repo/test_raw_query_outbound.sh: a socket outside dns_raw.rs.
async fn probe() { let _ = tokio::net::UdpSocket::bind("0.0.0.0:0").await; }
