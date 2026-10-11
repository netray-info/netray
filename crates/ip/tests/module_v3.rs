// V2 Phase 3, C2 (R3(a), IP): loading the module with the test config logs no
// "user_agent_regexes" line, and the module declares only `ip.reputation`.
//
// Cargo needs on netray-ip: `[[test]] name = "module_v3"`, `path = "tests/module_v3.rs"`,
// `required-features = ["testing"]`, and dev-dependency `tracing-subscriber` (already in the
// workspace via netray).

use std::io::Write;
use std::sync::{Arc, Mutex};

use netray_engine::Module;
use netray_ip::{IpModule, ModuleConfig};
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Capture {
    type Writer = Capture;
    fn make_writer(&'a self) -> Capture {
        self.clone()
    }
}

#[tokio::test]
async fn ip_module_load_is_silent_about_user_agents_and_declares_only_reputation() {
    let capture = Capture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(capture.clone())
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .finish();
    // current-thread runtime: the thread-local default sees every event of `new`.
    let guard = tracing::subscriber::set_default(subscriber);

    let cfg: ModuleConfig = toml::from_str("").unwrap();
    let module = IpModule::new(cfg).await.expect("module builds");
    drop(guard);

    let logged = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    assert!(
        !logged.contains("user_agent_regexes"),
        "module load logged a user-agent line:\n{logged}"
    );

    let ids: Vec<String> = module.checks().iter().map(|c| c.to_string()).collect();
    assert_eq!(ids, vec!["ip.reputation".to_string()]);
}
