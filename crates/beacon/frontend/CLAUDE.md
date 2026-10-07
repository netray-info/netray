# CLAUDE.md — beacon frontend
Apply [frontend-rules](../../specs/rules/frontend-rules.md) for all changes under frontend/.
The dist/ directory is embedded into the Rust binary via rust-embed at release build time.
npm install requires NODE_AUTH_TOKEN set to a GitHub PAT with read:packages scope
(for @netray-info/common-frontend from npm.pkg.github.com).
