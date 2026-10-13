// The HTTP section waits for the resolve inside its own window, both counted from the start of
// a check, so `resolve_timeout_ms` must stay below `[backends.http] timeout_ms` (C18), as it
// does for the tls and ip windows.

use lens::config::Config;

fn load_toml(contents: &str) -> Result<Config, impl std::fmt::Display> {
    let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
    std::io::Write::write_all(&mut file, contents.as_bytes()).unwrap();
    Config::load(Some(file.path().to_str().unwrap()))
}

#[test]
fn resolve_timeout_must_stay_below_the_http_timeout() {
    let toml = "[backends]\nresolve_timeout_ms = 1500\n\
                [backends.tls]\ntimeout_ms = 3000\n\
                [backends.http]\ntimeout_ms = 1500\n\
                [backends.ip]\ntimeout_ms = 3000\n";
    let msg = match load_toml(toml) {
        Ok(_) => panic!("resolve_timeout_ms == http timeout_ms must be rejected"),
        Err(e) => e.to_string(),
    };
    assert!(
        msg.contains("resolve_timeout_ms") && msg.contains("[backends.http] timeout_ms"),
        "error must name resolve_timeout_ms and the HTTP timeout, got: {msg}"
    );
    if let Err(e) = load_toml(&toml.replace("http]\ntimeout_ms = 1500", "http]\ntimeout_ms = 1501"))
    {
        panic!("one millisecond above the resolve budget must load: {e}");
    }
}
