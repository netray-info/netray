//! The V1 `netray lens` subcommand, kept runnable as the reference until Phase 7 deletes
//! `crates/lens` (spec v2-cutover, Decision log 2026-10-19). The only file of `crates/netray/src`
//! that names `lens` (`tests/repo/test_no_lens_dependency.sh`).

use crate::registry::{module_registry, reload_ip_on_sighup};

/// Removes the enrichment-backend keys `netray lens` still accepts in its old tables: the
/// modules enrich through the in-process ip module now. Warns when lens named no `ip_url` for the
/// http module and has no `[modules.ip]` either, as lens did before.
fn strip_lens_enrichment(cfg: &mut lens::config::Config) {
    let ip_url_set = cfg
        .modules
        .get("http")
        .and_then(|t| t.get("enrichment"))
        .and_then(|e| e.get("ip_url"))
        .is_some();
    if cfg.backends.http.is_some() && !ip_url_set && !cfg.modules.contains_key("ip") {
        tracing::warn!(
            "modules.http.enrichment.ip_url is not set: server org and network type stay empty"
        );
    }
    for (module, key) in [
        ("dns", "backends"),
        ("tls", "backends"),
        ("http", "enrichment"),
        ("email", "backends"),
    ] {
        if let Some(table) = cfg.modules.get_mut(module) {
            table.remove(key);
            if table.is_empty() {
                cfg.modules.remove(module);
            }
        }
    }
}

/// `netray lens --check-config <path>`: the lens config and its module tables, no data files.
pub async fn check_config(path: &str) -> Result<(), String> {
    match lens::config::Config::load(Some(path)) {
        Ok(mut cfg) => {
            strip_lens_enrichment(&mut cfg);
            module_registry(&cfg.modules, false).await.map(drop)
        }
        Err(e) => Err(e.to_string()),
    }
}

/// `netray lens [config]`: lens's application over the module registry.
pub async fn run(config: Option<String>) -> anyhow::Result<()> {
    let path = config.or_else(|| std::env::var("LENS_CONFIG").ok());
    let mut cfg = lens::config::Config::load(path.as_deref())
        .map_err(|e| anyhow::anyhow!("failed to load configuration: {e}"))?;
    lens::init_telemetry(&cfg);
    strip_lens_enrichment(&mut cfg);
    let (registry, ip, parts) = module_registry(&cfg.modules, true)
        .await
        .map_err(|e| anyhow::anyhow!(e))?
        .split();
    if let Some(ip) = ip {
        reload_ip_on_sighup(ip);
    }
    lens::run_with(path, registry, parts).await;
    Ok(())
}

/// Moved here from `crates/netray/tests/run_bytes_unchanged.rs` (v2-cutover Phase 6) so that
/// only this file names lens.
#[cfg(test)]
mod run_bytes_unchanged {
    //! The domain run copied into netray-engine (`netray_engine::domain_run`, spec v2-cutover
    //! Phase 6) stores the same `Run` bytes lens stores today, or every domain would record a
    //! changed entry after the cutover. One scripted run per scenario goes through lens's admission
    //! (its `Published` carries the run lens records) and through `netray_engine::run_domain` over
    //! an identical registry, lens's section plan, resolve budget and profile; the two runs must
    //! serialise to the same JSON, and the scores and section results must agree.
    //!
    //! Remove this file with `crates/lens` in Phase 7: it is the only reason a test here reads lens.

    use std::net::{IpAddr, Ipv4Addr};
    use std::path::PathBuf;
    use std::time::Duration;

    use lens::admission::{Admit, RunEvent, Trigger, admit};
    use lens::config::Config;
    use lens::state::AppState;
    use netray_engine::domain_run::{CheckInput, HARD_DEADLINE, run_domain};
    use netray_engine::{
        BoxFuture, Domain, EvidencePath, Facts, FactsProvider, Module, Registry, ResolveError,
        RunContext, SectionOutcome, section_plan,
    };
    use netray_model::evidence::{Block, BlockId, Shape};
    use netray_model::{CheckId, CheckResult, Protocol, Status};

    const PROTOCOLS: [Protocol; 5] = [
        Protocol::Dns,
        Protocol::Tls,
        Protocol::Http,
        Protocol::Email,
        Protocol::Ip,
    ];

    /// What a stub module does on its run.
    #[derive(Clone)]
    enum Plan {
        Answer(SectionOutcome),
        /// Never answers: the section's window closes on it.
        Hang,
    }

    struct Script {
        protocol: Protocol,
        plan: Plan,
    }

    impl Module for Script {
        fn protocol(&self) -> Protocol {
            self.protocol
        }
        fn checks(&self) -> &'static [CheckId] {
            &[]
        }
        fn volatile(&self) -> &'static [EvidencePath] {
            &[]
        }
        fn run<'a>(
            &'a self,
            _ctx: &'a RunContext,
            _facts: &'a Facts,
        ) -> BoxFuture<'a, SectionOutcome> {
            let plan = self.plan.clone();
            Box::pin(async move {
                match plan {
                    Plan::Answer(outcome) => outcome,
                    Plan::Hang => std::future::pending().await,
                }
            })
        }
    }

    #[derive(Clone)]
    struct StubFacts(Result<Facts, ResolveError>);

    impl FactsProvider for StubFacts {
        fn resolve<'a>(
            &'a self,
            _ctx: &'a RunContext,
            _domain: &'a Domain,
        ) -> BoxFuture<'a, Result<Facts, ResolveError>> {
            Box::pin(async move { self.0.clone() })
        }
    }

    fn text_block(id: &str) -> Block {
        Block {
            id: BlockId::parse(id).unwrap(),
            title: format!("block {id}"),
            shape: Shape::Text(format!("content of {id}")),
        }
    }

    /// A measured section of `protocol`: its first catalogue checks with every status in turn
    /// (`NotTested` and `NotApplicable` included), findings, evidence naming a block of the run and
    /// one the run lacks, and the section's own block.
    fn measured(protocol: Protocol) -> Plan {
        const STATUSES: [Status; 6] = [
            Status::Pass,
            Status::Warn,
            Status::Fail,
            Status::NotTested,
            Status::NotApplicable,
            Status::Unmeasured,
        ];
        let p = protocol.as_str();
        let checks = netray_engine::catalogue()
            .entries()
            .iter()
            .filter(|e| e.protocol == protocol)
            .zip(STATUSES.iter().cycle())
            .map(|(e, status)| CheckResult {
                id: e.id.clone(),
                status: *status,
                findings: vec![format!("{} says {status:?}", e.id.name())],
                evidence: vec![
                    BlockId::parse(&format!("{p}.own")).unwrap(),
                    BlockId::parse(&format!("{p}.ghost")).unwrap(),
                ],
            })
            .collect();
        Plan::Answer(SectionOutcome::Measured {
            checks,
            evidence: vec![text_block(&format!("{p}.own"))],
        })
    }

    fn all_pass(protocol: Protocol) -> Plan {
        let checks = netray_engine::catalogue()
            .entries()
            .iter()
            .filter(|e| e.protocol == protocol)
            .map(|e| CheckResult {
                id: e.id.clone(),
                status: Status::Pass,
                findings: vec![],
                evidence: vec![],
            })
            .collect();
        Plan::Answer(SectionOutcome::Measured {
            checks,
            evidence: vec![],
        })
    }

    /// Facts answering one address, with a block the DNS section also carries and one it lacks.
    fn facts_with_address() -> Result<Facts, ResolveError> {
        Ok(Facts {
            a: vec!["192.0.2.1".parse().unwrap()],
            blocks: vec![text_block("dns.own"), text_block("dns.mx")],
            ..Facts::default()
        })
    }

    struct Scenario {
        name: &'static str,
        plans: Vec<(Protocol, Plan)>,
        facts: Result<Facts, ResolveError>,
    }

    fn scenarios() -> Vec<Scenario> {
        let every =
            |f: fn(Protocol) -> Plan| PROTOCOLS.iter().map(|p| (*p, f(*p))).collect::<Vec<_>>();
        let with = |mut plans: Vec<(Protocol, Plan)>, p: Protocol, plan: Plan| {
            plans.iter_mut().find(|(q, _)| *q == p).unwrap().1 = plan;
            plans
        };
        vec![
            Scenario {
                name: "every section measured, mixed statuses, facts on dns",
                plans: every(measured),
                facts: facts_with_address(),
            },
            Scenario {
                name: "complete all-pass run",
                plans: every(all_pass),
                facts: facts_with_address(),
            },
            Scenario {
                name: "dns incomplete, email not applicable: facts outside every section",
                plans: with(
                    with(
                        every(all_pass),
                        Protocol::Dns,
                        Plan::Answer(SectionOutcome::Incomplete {
                            reason: "module failed".into(),
                        }),
                    ),
                    Protocol::Email,
                    Plan::Answer(SectionOutcome::NotApplicable {
                        reason: "no MX record".into(),
                    }),
                ),
                facts: facts_with_address(),
            },
            Scenario {
                name: "email hangs past its window, no address: ip has no dns results",
                plans: with(every(all_pass), Protocol::Email, Plan::Hang),
                facts: Ok(Facts::default()),
            },
            Scenario {
                name: "definite no-address answer",
                plans: every(measured),
                facts: Ok(Facts {
                    no_address_definite: true,
                    ..Facts::default()
                }),
            },
            Scenario {
                name: "failed resolve",
                plans: every(measured),
                facts: Err(ResolveError("resolver unreachable".into())),
            },
        ]
    }

    fn registry(s: &Scenario) -> Registry {
        s.plans
            .iter()
            .fold(Registry::new(), |r, (protocol, plan)| {
                r.with(Box::new(Script {
                    protocol: *protocol,
                    plan: plan.clone(),
                }))
            })
            .with_facts(Box::new(StubFacts(s.facts.clone())))
    }

    fn lens_state(registry: Registry) -> AppState {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../lens/tests/fixtures/lens.production.toml");
        let mut config = Config::load(fixture.to_str()).expect("production config loads");
        config.cache.enabled = false;
        config.snapshots.enabled = false;
        config.history.enabled = false;
        config.rate_limit.per_ip_per_minute = 1000;
        config.rate_limit.per_ip_burst = 1000;
        config.backends.email.as_mut().unwrap().timeout_ms = 300;
        AppState::with_registry(config, registry).unwrap()
    }

    /// A section result in a form both crates' types print alike.
    fn section_debug<R, E: std::fmt::Debug>(
        result: &Result<R, E>,
        checks: impl Fn(&R) -> String,
    ) -> String {
        match result {
            Ok(r) => format!("Ok({})", checks(r)),
            Err(e) => format!("Err({e:?})"),
        }
    }

    #[tokio::test]
    async fn the_engine_domain_run_stores_the_bytes_lens_stores() {
        for (i, s) in scenarios().iter().enumerate() {
            let domain = format!("s{i}.example.com");
            let state = lens_state(registry(s));

            let client = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1 + i as u8));
            let Admit::Admitted(mut stream) = admit(&state, &domain, client, Trigger::Visit, false)
            else {
                panic!("{}: the lens run was not admitted", s.name);
            };
            let mut published = None;
            while let Some(event) = stream.next().await {
                if let RunEvent::Done(p) = event {
                    published = Some(p);
                }
            }
            let published =
                published.unwrap_or_else(|| panic!("{}: lens published no run", s.name));

            let registry = registry(s);
            let plan: Vec<(Protocol, Duration)> = state
                .backends
                .iter()
                .map(|b| (b.protocol, b.timeout))
                .collect();
            let b = &state.config.backends;
            let windows = section_plan(&registry, |p| {
                let ms = match p {
                    Protocol::Dns => Some(b.dns.timeout_ms),
                    Protocol::Tls => Some(b.tls.timeout_ms),
                    Protocol::Http => b.http.as_ref().map(|c| c.timeout_ms),
                    Protocol::Email => b.email.as_ref().map(|c| c.timeout_ms),
                    Protocol::Ip => Some(b.ip.timeout_ms),
                };
                ms.map(Duration::from_millis)
            });
            assert_eq!(windows, plan, "{}: section_plan builds lens's plan", s.name);

            let got = run_domain(
                &registry,
                &plan,
                Duration::from_millis(b.resolve_timeout_ms),
                &state.scoring_profile,
                CheckInput {
                    domain: domain.clone(),
                    dkim_selectors: None,
                },
                HARD_DEADLINE,
                |_| {},
            )
            .await;

            assert_eq!(
                serde_json::to_string(&got.run).unwrap(),
                serde_json::to_string(&published.run).unwrap(),
                "{}: stored run bytes differ",
                s.name
            );
            assert_eq!(got.facts, published.facts, "{}: facts", s.name);

            let (new, old) = (&got.output.score, &published.output.score);
            assert_eq!(new.grade, old.grade, "{}: grade", s.name);
            assert_eq!(new.complete, old.complete, "{}: complete", s.name);
            assert_eq!(
                new.overall_percentage, old.overall_percentage,
                "{}: percentage",
                s.name
            );
            assert_eq!(new.hard_fail_checks, old.hard_fail_checks, "{}", s.name);
            assert_eq!(new.not_applicable, old.not_applicable, "{}", s.name);
            assert_eq!(
                format!("{:?}", sorted(&new.sections)),
                format!("{:?}", sorted(&old.sections)),
                "{}: section scores",
                s.name
            );

            let mut names: Vec<&String> = published.output.sections.keys().collect();
            names.sort();
            let mut new_names: Vec<&String> = got.output.sections.keys().collect();
            new_names.sort();
            assert_eq!(new_names, names, "{}: section names", s.name);
            for name in names {
                let old = section_debug(&published.output.sections[name], |r| {
                    format!("{:?} {:?}", checks_of(&r.checks), r.evidence)
                });
                let new = section_debug(&got.output.sections[name], |r| {
                    format!("{:?} {:?}", new_checks_of(&r.checks), r.evidence)
                });
                assert_eq!(new, old, "{}: section {name}", s.name);
            }
        }
    }

    fn sorted<V>(m: &std::collections::HashMap<String, V>) -> Vec<(&String, &V)> {
        let mut v: Vec<_> = m.iter().collect();
        v.sort_by_key(|(k, _)| *k);
        v
    }

    type CheckRow = (String, String, Vec<String>, Vec<BlockId>);

    fn checks_of(checks: &[lens::score::CheckResult]) -> Vec<CheckRow> {
        checks
            .iter()
            .map(|c| {
                (
                    c.name.clone(),
                    format!("{:?}", c.verdict),
                    c.messages.clone(),
                    c.evidence.clone(),
                )
            })
            .collect()
    }

    fn new_checks_of(checks: &[netray_engine::domain_run::CheckResult]) -> Vec<CheckRow> {
        checks
            .iter()
            .map(|c| {
                (
                    c.name.clone(),
                    format!("{:?}", c.verdict),
                    c.messages.clone(),
                    c.evidence.clone(),
                )
            })
            .collect()
    }
}
