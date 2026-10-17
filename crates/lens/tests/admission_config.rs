// The `[admission]` table (specs/features/v2-admission, Phase 1, requirement 1): defaults,
// validation of `domain_run_slots`, strict keys, the `LENS_ADMISSION__` override, and the
// unchanged `[rate_limit]` defaults.

use lens::config::Config;

fn load_toml(contents: &str) -> Result<Config, impl std::fmt::Display> {
    let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
    std::io::Write::write_all(&mut file, contents.as_bytes()).unwrap();
    Config::load(Some(file.path().to_str().unwrap()))
}

fn load_ok(contents: &str) -> Config {
    match load_toml(contents) {
        Ok(c) => c,
        Err(e) => panic!("config must load: {e}\n---\n{contents}"),
    }
}

fn load_err(contents: &str) -> String {
    match load_toml(contents) {
        Ok(_) => panic!("config must be refused:\n{contents}"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn admission_table_loading_and_env_override() {
    // C1: defaults without the table.
    let cfg = load_ok("");
    assert_eq!(cfg.admission.client_budget_per_hour, 30);
    assert_eq!(cfg.admission.domain_run_slots, 8);

    // C2: zero run slots is refused, naming the key.
    let msg = load_err("[admission]\ndomain_run_slots = 0\n");
    assert!(
        msg.contains("admission.domain_run_slots"),
        "error must name admission.domain_run_slots, got: {msg}"
    );

    // C3: boundary values load as written.
    let cfg = load_ok("[admission]\nclient_budget_per_hour = 0\n");
    assert_eq!(cfg.admission.client_budget_per_hour, 0);
    let cfg = load_ok("[admission]\nclient_budget_per_hour = 1\n");
    assert_eq!(cfg.admission.client_budget_per_hour, 1);
    let cfg = load_ok("[admission]\ndomain_run_slots = 1\n");
    assert_eq!(cfg.admission.domain_run_slots, 1);

    // C5: an unknown key is rejected and named.
    let msg = load_err("[admission]\nburst_size = 5\n");
    assert!(
        msg.contains("burst_size"),
        "error must name the unknown key, got: {msg}"
    );

    // C7: [rate_limit] defaults are unchanged.
    let cfg = load_ok("");
    assert_eq!(cfg.rate_limit.per_ip_per_minute, 10);
    assert_eq!(cfg.rate_limit.per_ip_burst, 3);
    assert_eq!(cfg.rate_limit.global_per_minute, 100);
    assert_eq!(cfg.rate_limit.global_burst, 20);

    // C4: the env override. Kept sequential in this one test: the others would race on the
    // process environment.
    // SAFETY: no other test in this binary reads or writes the environment concurrently.
    unsafe { std::env::set_var("LENS_ADMISSION__CLIENT_BUDGET_PER_HOUR", "0") };
    let loaded = load_toml("");
    unsafe { std::env::remove_var("LENS_ADMISSION__CLIENT_BUDGET_PER_HOUR") };
    let cfg = match loaded {
        Ok(c) => c,
        Err(e) => panic!("config with env override must load: {e}"),
    };
    assert_eq!(cfg.admission.client_budget_per_hour, 0);
}
