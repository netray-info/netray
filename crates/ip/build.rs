use std::path::Path;

fn main() {
    println!("cargo::rerun-if-changed=frontend/dist");
    // Rerun when this checkout's commit changes.
    // Ask git where they are (a worktree splits HEAD from the shared refs) and name only paths that
    // exist: a missing path makes cargo rerun the script on every call. Not all of refs/heads: a
    // commit on another branch (another worktree) must not rebuild this one. Without a git checkout
    // (Docker build) nothing is named.
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let path_of = |name: &str| {
        git(&["rev-parse", "--path-format=absolute", "--git-path", name]).filter(|p| Path::new(p).exists())
    };
    // HEAD and its reflog are this checkout's own (a commit, checkout or reset appends to logs/HEAD,
    // also when the branch ref is packed); the branch's loose ref when there is one.
    let mut watched: Vec<String> = ["HEAD", "logs/HEAD"].iter().filter_map(|n| path_of(n)).collect();
    if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
        watched.extend(path_of(&branch));
    }
    for path in watched {
        println!("cargo::rerun-if-changed={path}");
    }

    let git_sha = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo::rustc-env=GIT_SHORT_SHA={git_sha}");

    let git_date = std::process::Command::new("git")
        .args(["log", "-1", "--format=%cd", "--date=short"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo::rustc-env=BUILD_DATE={git_date}");

    let dist = Path::new("frontend/dist/index.html");
    if !dist.exists() {
        if cfg!(debug_assertions) {
            // Create a placeholder for debug builds
            let dir = Path::new("frontend/dist");
            std::fs::create_dir_all(dir).expect("Failed to create frontend/dist");
            std::fs::write(
                dist,
                "<!DOCTYPE html><html><body><p>Run <code>cd frontend &amp;&amp; npm run build</code> for the real SPA.</p></body></html>",
            )
            .expect("Failed to write placeholder index.html");
        } else {
            panic!("frontend/dist/index.html not found — run `cd frontend && npm run build` before a release build");
        }
    }
}
