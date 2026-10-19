// C6: the in-process IP module parses the caller's User-Agent from a `user_agent_regexes` file
// named in `[modules.ip]`, as V1 ifconfig did. Runs offline: no GeoIP data.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use netray_ip::{IpLookup, IpModule, ModuleConfig};

const FIREFOX: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0";

fn fixture() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ua_regexes.yaml")
        .to_string_lossy()
        .into_owned()
}

#[test]
fn module_config_accepts_user_agent_regexes() {
    let cfg: ModuleConfig = toml::from_str(r#"user_agent_regexes = "/data/regexes.yaml""#)
        .expect("user_agent_regexes is a [modules.ip] key");
    // Round-trips through the module config only; the value is read by the module at load.
    let _ = cfg;
}

#[tokio::test]
async fn caller_user_agent_is_parsed_from_the_configured_regexes() {
    let cfg: ModuleConfig = toml::from_str(&format!("user_agent_regexes = {:?}", fixture()))
        .expect("user_agent_regexes is a [modules.ip] key");
    let module = IpModule::new(cfg).await.expect("module loads without GeoIP data");
    let remote = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)), 4242);
    let answer = module.lookup_caller(remote, Some(FIREFOX)).await.expect("caller answers");
    let ua = answer.user_agent.expect("the caller carries a user agent");
    assert_eq!(ua.browser.family, "Firefox");
    assert_eq!(ua.browser.major.as_deref(), Some("128"));
    assert_eq!(ua.os.family, "Linux");
    assert_eq!(ua.raw.as_deref(), Some(FIREFOX));
}
