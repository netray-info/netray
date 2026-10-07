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
}
