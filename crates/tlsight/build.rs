fn main() {
    let dist = std::path::Path::new("frontend/dist/index.html");
    if !dist.exists() {
        #[cfg(debug_assertions)]
        {
            let dir = std::path::Path::new("frontend/dist");
            std::fs::create_dir_all(dir).expect("failed to create frontend/dist");
            std::fs::write(
                dist,
                r#"<!DOCTYPE html><html><body><p>Run <code>cd frontend && npm run dev</code> for the dev server.</p></body></html>"#,
            )
            .expect("failed to write placeholder index.html");
        }
        #[cfg(not(debug_assertions))]
        {
            panic!(
                "\n\nerror: frontend/dist/index.html not found.\n\
                 Run 'cd frontend && npm ci && npm run build' before 'cargo build --release'.\n\n"
            );
        }
    }
    println!("cargo::rerun-if-changed=frontend/dist");

    generate_caa_issuers();
}

fn generate_caa_issuers() {
    println!("cargo::rerun-if-changed=data/caa_domains.tsv");

    let tsv_path = std::path::Path::new("data/caa_domains.tsv");
    let content = std::fs::read_to_string(tsv_path)
        .expect("data/caa_domains.tsv not found; run 'make data' to fetch and process CA data");

    let mut entries: Vec<(&str, &str)> = content
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(2, '\t');
            let domain = parts.next()?.trim();
            let name = parts.next()?.trim();
            if domain.is_empty() || name.is_empty() {
                None
            } else {
                Some((domain, name))
            }
        })
        .collect();

    // process.py outputs sorted, but enforce it here so binary search is correct.
    entries.sort_by_key(|&(d, _)| d);

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_path = std::path::Path::new(&out_dir).join("caa_issuers.rs");

    let mut code = String::from(
        "// Auto-generated from data/caa_domains.tsv — do not edit.\n\
         // Regenerate with: make data\n\n\
         pub static CAA_ISSUERS: &[(&str, &str)] = &[\n",
    );
    for (domain, name) in &entries {
        code.push_str(&format!("    ({domain:?}, {name:?}),\n"));
    }
    code.push_str(
        "];\n\n\
         /// Look up the CA display name for a CAA `issue` domain value.\n\
         /// The slice is sorted by domain, enabling binary search.\n\
         pub fn lookup_caa_issuer(caa_domain: &str) -> Option<&'static str> {\n\
             CAA_ISSUERS\n\
                 .binary_search_by_key(&caa_domain, |&(d, _)| d)\n\
                 .ok()\n\
                 .map(|i| CAA_ISSUERS[i].1)\n\
         }\n",
    );

    std::fs::write(&out_path, code).expect("failed to write caa_issuers.rs");
}
