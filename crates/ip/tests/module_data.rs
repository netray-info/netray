// V2 Phase 3: the IP module keeps ifconfig-rs's load semantics (a missing optional list or an
// absent GeoIP database is not an error); lens's "data required when [modules.ip] is
// configured" rule is enforced in the `netray` binary, see tests/repo/test_check_config.sh.
// Needs `toml` as a dev-dependency of netray-ip. Runs offline.

use netray_ip::{IpModule, ModuleConfig};

#[tokio::test]
async fn missing_reputation_list_without_geoip_dbs_still_loads() {
    let module: ModuleConfig = toml::from_str(r#"feodo_botnet_ips = "/nonexistent.txt""#).unwrap();
    assert!(module.geoip_city_db.is_none() && module.geoip_asn_db.is_none());
    IpModule::new(module)
        .await
        .expect("a missing optional list only warns at the module level");
}

#[test]
fn module_config_refuses_user_agent_regexes() {
    let err = toml::from_str::<ModuleConfig>(r#"user_agent_regexes = "/nonexistent.yaml""#)
        .expect_err("lens never reads user_agent_regexes; the key must be unknown");
    assert!(err.to_string().contains("user_agent_regexes"), "{err}");
}
